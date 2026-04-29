import { createRequire } from "node:module";
import { Cli, Command, Option } from "clipanion";
import path, { dirname, isAbsolute, join, parse, resolve } from "node:path";
import * as colors from "colorette";
import { underline, yellow } from "colorette";
import { createDebug } from "obug";
import { access, copyFile, mkdir, readFile, readdir, rename, stat, unlink, writeFile } from "node:fs/promises";
import { exec, execSync, spawn, spawnSync } from "node:child_process";
import fs, { existsSync, mkdirSync, promises, rmSync, statSync } from "node:fs";
import { isNil, merge, omit, omitBy, pick, sortBy } from "es-toolkit";
import { createHash } from "node:crypto";
import { homedir } from "node:os";
import { parse as parse$1 } from "semver";
import { dump, load } from "js-yaml";
import * as typanion from "typanion";
import { Octokit } from "@octokit/rest";
import { checkbox, confirm, input, select } from "@inquirer/prompts";

//#region src/def/artifacts.ts
var BaseArtifactsCommand = class extends Command {
	static paths = [["artifacts"]];
	static usage = Command.Usage({ description: "Copy artifacts from Github Actions into npm packages and ready to publish" });
	cwd = Option.String("--cwd", process.cwd(), { description: "The working directory of where napi command will be executed in, all other paths options are relative to this path" });
	configPath = Option.String("--config-path,-c", { description: "Path to `napi` config json file" });
	packageJsonPath = Option.String("--package-json-path", "package.json", { description: "Path to `package.json`" });
	outputDir = Option.String("--output-dir,-o,-d", "./artifacts", { description: "Path to the folder where all built `.node` files put, same as `--output-dir` of build command" });
	npmDir = Option.String("--npm-dir", "npm", { description: "Path to the folder where the npm packages put" });
	buildOutputDir = Option.String("--build-output-dir", { description: "Path to the build output dir, only needed when targets contains `wasm32-wasi-*`" });
	getOptions() {
		return {
			cwd: this.cwd,
			configPath: this.configPath,
			packageJsonPath: this.packageJsonPath,
			outputDir: this.outputDir,
			npmDir: this.npmDir,
			buildOutputDir: this.buildOutputDir
		};
	}
};
function applyDefaultArtifactsOptions(options) {
	return {
		cwd: process.cwd(),
		packageJsonPath: "package.json",
		outputDir: "./artifacts",
		npmDir: "npm",
		...options
	};
}

//#endregion
//#region src/utils/log.ts
const debugFactory = (namespace) => {
	const debug$10 = createDebug(`napi:${namespace}`, { formatters: { i(v) {
		return colors.green(v);
	} } });
	debug$10.info = (...args) => console.error(colors.black(colors.bgGreen(" INFO ")), ...args);
	debug$10.warn = (...args) => console.error(colors.black(colors.bgYellow(" WARNING ")), ...args);
	debug$10.error = (...args) => console.error(colors.white(colors.bgRed(" ERROR ")), ...args.map((arg) => arg instanceof Error ? arg.stack ?? arg.message : arg));
	return debug$10;
};
const debug$9 = debugFactory("utils");

//#endregion
//#region package.json
var version$1 = "3.5.1";

//#endregion
//#region src/utils/misc.ts
const readFileAsync = readFile;
const writeFileAsync = writeFile;
const unlinkAsync = unlink;
const copyFileAsync = copyFile;
const mkdirAsync = mkdir;
const statAsync = stat;
const readdirAsync = readdir;
function fileExists(path$1) {
	return access(path$1).then(() => true, () => false);
}
async function dirExistsAsync(path$1) {
	try {
		return (await statAsync(path$1)).isDirectory();
	} catch {
		return false;
	}
}
function pick$1(o, ...keys) {
	return keys.reduce((acc, key) => {
		acc[key] = o[key];
		return acc;
	}, {});
}
async function updatePackageJson(path$1, partial) {
	if (!await fileExists(path$1)) {
		debug$9(`File not exists ${path$1}`);
		return;
	}
	const old = JSON.parse(await readFileAsync(path$1, "utf8"));
	await writeFileAsync(path$1, JSON.stringify({
		...old,
		...partial
	}, null, 2));
}
const CLI_VERSION = version$1;

//#endregion
//#region src/utils/target.ts
const SUB_SYSTEMS = new Set(["android", "ohos"]);
const AVAILABLE_TARGETS = [
	"aarch64-apple-darwin",
	"aarch64-linux-android",
	"aarch64-unknown-linux-gnu",
	"aarch64-unknown-linux-musl",
	"aarch64-unknown-linux-ohos",
	"aarch64-pc-windows-msvc",
	"x86_64-apple-darwin",
	"x86_64-pc-windows-msvc",
	"x86_64-pc-windows-gnu",
	"x86_64-unknown-linux-gnu",
	"x86_64-unknown-linux-musl",
	"x86_64-unknown-linux-ohos",
	"x86_64-unknown-freebsd",
	"i686-pc-windows-msvc",
	"armv7-unknown-linux-gnueabihf",
	"armv7-unknown-linux-musleabihf",
	"armv7-linux-androideabi",
	"universal-apple-darwin",
	"loongarch64-unknown-linux-gnu",
	"riscv64gc-unknown-linux-gnu",
	"powerpc64le-unknown-linux-gnu",
	"s390x-unknown-linux-gnu",
	"wasm32-wasi-preview1-threads",
	"wasm32-wasip1-threads"
];
const DEFAULT_TARGETS = [
	"x86_64-apple-darwin",
	"aarch64-apple-darwin",
	"x86_64-pc-windows-msvc",
	"x86_64-unknown-linux-gnu"
];
const TARGET_LINKER = {
	"aarch64-unknown-linux-musl": "aarch64-linux-musl-gcc",
	"loongarch64-unknown-linux-gnu": "loongarch64-linux-gnu-gcc-13",
	"riscv64gc-unknown-linux-gnu": "riscv64-linux-gnu-gcc",
	"powerpc64le-unknown-linux-gnu": "powerpc64le-linux-gnu-gcc",
	"s390x-unknown-linux-gnu": "s390x-linux-gnu-gcc"
};
const CpuToNodeArch = {
	x86_64: "x64",
	aarch64: "arm64",
	i686: "ia32",
	armv7: "arm",
	loongarch64: "loong64",
	riscv64gc: "riscv64",
	powerpc64le: "ppc64"
};
const SysToNodePlatform = {
	linux: "linux",
	freebsd: "freebsd",
	darwin: "darwin",
	windows: "win32",
	ohos: "openharmony"
};
const UniArchsByPlatform = { darwin: ["x64", "arm64"] };
/**
* A triple is a specific format for specifying a target architecture.
* Triples may be referred to as a target triple which is the architecture for the artifact produced, and the host triple which is the architecture that the compiler is running on.
* The general format of the triple is `<arch><sub>-<vendor>-<sys>-<abi>` where:
*   - `arch` = The base CPU architecture, for example `x86_64`, `i686`, `arm`, `thumb`, `mips`, etc.
*   - `sub` = The CPU sub-architecture, for example `arm` has `v7`, `v7s`, `v5te`, etc.
*   - `vendor` = The vendor, for example `unknown`, `apple`, `pc`, `nvidia`, etc.
*   - `sys` = The system name, for example `linux`, `windows`, `darwin`, etc. none is typically used for bare-metal without an OS.
*   - `abi` = The ABI, for example `gnu`, `android`, `eabi`, etc.
*/
function parseTriple(rawTriple) {
	if (rawTriple === "wasm32-wasi" || rawTriple === "wasm32-wasi-preview1-threads" || rawTriple.startsWith("wasm32-wasip")) return {
		triple: rawTriple,
		platformArchABI: "wasm32-wasi",
		platform: "wasi",
		arch: "wasm32",
		abi: "wasi"
	};
	const triples = (rawTriple.endsWith("eabi") ? `${rawTriple.slice(0, -4)}-eabi` : rawTriple).split("-");
	let cpu;
	let sys;
	let abi = null;
	if (triples.length === 2) [cpu, sys] = triples;
	else [cpu, , sys, abi = null] = triples;
	if (abi && SUB_SYSTEMS.has(abi)) {
		sys = abi;
		abi = null;
	}
	const platform = SysToNodePlatform[sys] ?? sys;
	const arch = CpuToNodeArch[cpu] ?? cpu;
	return {
		triple: rawTriple,
		platformArchABI: abi ? `${platform}-${arch}-${abi}` : `${platform}-${arch}`,
		platform,
		arch,
		abi
	};
}
function getSystemDefaultTarget() {
	const host = execSync(`rustc -vV`, { env: process.env }).toString("utf8").split("\n").find((line) => line.startsWith("host: "));
	const triple = host === null || host === void 0 ? void 0 : host.slice(6);
	if (!triple) throw new TypeError(`Can not parse target triple from host`);
	return parseTriple(triple);
}
function getTargetLinker(target) {
	return TARGET_LINKER[target];
}
function targetToEnvVar(target) {
	return target.replace(/-/g, "_").toUpperCase();
}

//#endregion
//#region src/utils/version.ts
let NapiVersion = /* @__PURE__ */ function(NapiVersion$1) {
	NapiVersion$1[NapiVersion$1["Napi1"] = 1] = "Napi1";
	NapiVersion$1[NapiVersion$1["Napi2"] = 2] = "Napi2";
	NapiVersion$1[NapiVersion$1["Napi3"] = 3] = "Napi3";
	NapiVersion$1[NapiVersion$1["Napi4"] = 4] = "Napi4";
	NapiVersion$1[NapiVersion$1["Napi5"] = 5] = "Napi5";
	NapiVersion$1[NapiVersion$1["Napi6"] = 6] = "Napi6";
	NapiVersion$1[NapiVersion$1["Napi7"] = 7] = "Napi7";
	NapiVersion$1[NapiVersion$1["Napi8"] = 8] = "Napi8";
	NapiVersion$1[NapiVersion$1["Napi9"] = 9] = "Napi9";
	return NapiVersion$1;
}({});
const NAPI_VERSION_MATRIX = new Map([
	[NapiVersion.Napi1, "8.6.0 | 9.0.0 | 10.0.0"],
	[NapiVersion.Napi2, "8.10.0 | 9.3.0 | 10.0.0"],
	[NapiVersion.Napi3, "6.14.2 | 8.11.2 | 9.11.0 | 10.0.0"],
	[NapiVersion.Napi4, "10.16.0 | 11.8.0 | 12.0.0"],
	[NapiVersion.Napi5, "10.17.0 | 12.11.0 | 13.0.0"],
	[NapiVersion.Napi6, "10.20.0 | 12.17.0 | 14.0.0"],
	[NapiVersion.Napi7, "10.23.0 | 12.19.0 | 14.12.0 | 15.0.0"],
	[NapiVersion.Napi8, "12.22.0 | 14.17.0 | 15.12.0 | 16.0.0"],
	[NapiVersion.Napi9, "18.17.0 | 20.3.0 | 21.1.0"]
]);
function parseNodeVersion(v) {
	const matches = v.match(/v?([0-9]+)\.([0-9]+)\.([0-9]+)/i);
	if (!matches) throw new Error("Unknown node version number: " + v);
	const [, major, minor, patch] = matches;
	return {
		major: parseInt(major),
		minor: parseInt(minor),
		patch: parseInt(patch)
	};
}
function requiredNodeVersions(napiVersion) {
	const requirement = NAPI_VERSION_MATRIX.get(napiVersion);
	if (!requirement) return [parseNodeVersion("10.0.0")];
	return requirement.split("|").map(parseNodeVersion);
}
function toEngineRequirement(versions) {
	const requirements = [];
	versions.forEach((v, i) => {
		let req = "";
		if (i !== 0) {
			const lastVersion = versions[i - 1];
			req += `< ${lastVersion.major + 1}`;
		}
		req += `${i === 0 ? "" : " || "}>= ${v.major}.${v.minor}.${v.patch}`;
		requirements.push(req);
	});
	return requirements.join(" ");
}
function napiEngineRequirement(napiVersion) {
	return toEngineRequirement(requiredNodeVersions(napiVersion));
}

//#endregion
//#region src/utils/metadata.ts
async function parseMetadata(manifestPath) {
	if (!fs.existsSync(manifestPath)) throw new Error(`No crate found in manifest: ${manifestPath}`);
	const childProcess = spawn("cargo", [
		"metadata",
		"--manifest-path",
		manifestPath,
		"--format-version",
		"1"
	], { stdio: "pipe" });
	let stdout = "";
	let stderr = "";
	let status = 0;
	childProcess.stdout.on("data", (data) => {
		stdout += data;
	});
	childProcess.stderr.on("data", (data) => {
		stderr += data;
	});
	await new Promise((resolve$1) => {
		childProcess.on("close", (code) => {
			status = code ?? 0;
			resolve$1();
		});
	});
	if (status !== 0) {
		const simpleMessage = `cargo metadata exited with code ${status}`;
		throw new Error(`${simpleMessage} and error message:\n\n${stderr}`, { cause: new Error(simpleMessage) });
	}
	try {
		return JSON.parse(stdout);
	} catch (e) {
		throw new Error("Failed to parse cargo metadata JSON", { cause: e });
	}
}

//#endregion
//#region src/utils/config.ts
async function readNapiConfig(path$1, configPath) {
	if (configPath && !await fileExists(configPath)) throw new Error(`NAPI-RS config not found at ${configPath}`);
	if (!await fileExists(path$1)) throw new Error(`package.json not found at ${path$1}`);
	const content = await readFileAsync(path$1, "utf8");
	let pkgJson;
	try {
		pkgJson = JSON.parse(content);
	} catch (e) {
		throw new Error(`Failed to parse package.json at ${path$1}`, { cause: e });
	}
	let separatedConfig;
	if (configPath) {
		const configContent = await readFileAsync(configPath, "utf8");
		try {
			separatedConfig = JSON.parse(configContent);
		} catch (e) {
			throw new Error(`Failed to parse NAPI-RS config at ${configPath}`, { cause: e });
		}
	}
	const userNapiConfig = pkgJson.napi ?? {};
	if (pkgJson.napi && separatedConfig) {
		const pkgJsonPath = underline(path$1);
		const configPathUnderline = underline(configPath);
		console.warn(yellow(`Both napi field in ${pkgJsonPath} and [NAPI-RS config](${configPathUnderline}) file are found, the NAPI-RS config file will be used.`));
	}
	if (separatedConfig) Object.assign(userNapiConfig, separatedConfig);
	const napiConfig = merge({
		binaryName: "index",
		packageName: pkgJson.name,
		targets: [],
		packageJson: pkgJson,
		npmClient: "npm"
	}, omit(userNapiConfig, ["targets"]));
	let targets = userNapiConfig.targets ?? [];
	if (userNapiConfig === null || userNapiConfig === void 0 ? void 0 : userNapiConfig.name) {
		console.warn(yellow(`[DEPRECATED] napi.name is deprecated, use napi.binaryName instead.`));
		napiConfig.binaryName = userNapiConfig.name;
	}
	if (!targets.length) {
		var _userNapiConfig$tripl, _userNapiConfig$tripl2;
		let deprecatedWarned = false;
		const warning = yellow(`[DEPRECATED] napi.triples is deprecated, use napi.targets instead.`);
		if ((_userNapiConfig$tripl = userNapiConfig.triples) === null || _userNapiConfig$tripl === void 0 ? void 0 : _userNapiConfig$tripl.defaults) {
			deprecatedWarned = true;
			console.warn(warning);
			targets = targets.concat(DEFAULT_TARGETS);
		}
		if ((_userNapiConfig$tripl2 = userNapiConfig.triples) === null || _userNapiConfig$tripl2 === void 0 || (_userNapiConfig$tripl2 = _userNapiConfig$tripl2.additional) === null || _userNapiConfig$tripl2 === void 0 ? void 0 : _userNapiConfig$tripl2.length) {
			targets = targets.concat(userNapiConfig.triples.additional);
			if (!deprecatedWarned) console.warn(warning);
		}
	}
	if (new Set(targets).size !== targets.length) {
		const duplicateTarget = targets.find((target, index) => targets.indexOf(target) !== index);
		throw new Error(`Duplicate targets are not allowed: ${duplicateTarget}`);
	}
	napiConfig.targets = targets.map(parseTriple);
	return napiConfig;
}

//#endregion
//#region src/utils/cargo.ts
function tryInstallCargoBinary(name, bin) {
	if (detectCargoBinary(bin)) {
		debug$9("Cargo binary already installed: %s", name);
		return;
	}
	try {
		debug$9("Installing cargo binary: %s", name);
		execSync(`cargo install ${name}`, { stdio: "inherit" });
	} catch (e) {
		throw new Error(`Failed to install cargo binary: ${name}`, { cause: e });
	}
}
function detectCargoBinary(bin) {
	debug$9("Detecting cargo binary: %s", bin);
	try {
		execSync(`cargo help ${bin}`, { stdio: "ignore" });
		debug$9("Cargo binary detected: %s", bin);
		return true;
	} catch {
		debug$9("Cargo binary not detected: %s", bin);
		return false;
	}
}

//#endregion
//#region src/utils/typegen.ts
const TOP_LEVEL_NAMESPACE = "__TOP_LEVEL_MODULE__";
const DEFAULT_TYPE_DEF_HEADER = `/* auto-generated by NAPI-RS */
/* eslint-disable */
`;
var TypeDefKind = /* @__PURE__ */ function(TypeDefKind$1) {
	TypeDefKind$1["Const"] = "const";
	TypeDefKind$1["Enum"] = "enum";
	TypeDefKind$1["StringEnum"] = "string_enum";
	TypeDefKind$1["Interface"] = "interface";
	TypeDefKind$1["Type"] = "type";
	TypeDefKind$1["Fn"] = "fn";
	TypeDefKind$1["Struct"] = "struct";
	TypeDefKind$1["Extends"] = "extends";
	TypeDefKind$1["Impl"] = "impl";
	return TypeDefKind$1;
}(TypeDefKind || {});
function prettyPrint(line, constEnum, ident, ambient = false) {
	let s = line.js_doc ?? "";
	switch (line.kind) {
		case TypeDefKind.Interface:
			s += `export interface ${line.name} {\n${line.def}\n}`;
			break;
		case TypeDefKind.Type:
			s += `export type ${line.name} = \n${line.def}`;
			break;
		case TypeDefKind.Enum:
			const enumName = constEnum ? "const enum" : "enum";
			s += `${exportDeclare(ambient)} ${enumName} ${line.name} {\n${line.def}\n}`;
			break;
		case TypeDefKind.StringEnum:
			if (constEnum) s += `${exportDeclare(ambient)} const enum ${line.name} {\n${line.def}\n}`;
			else s += `export type ${line.name} = ${line.def.replaceAll(/.*=/g, "").replaceAll(",", "|")};`;
			break;
		case TypeDefKind.Struct:
			const extendsDef = line.extends ? ` extends ${line.extends}` : "";
			if (line.extends) {
				const genericMatch = line.extends.match(/Iterator<(.+)>$/);
				if (genericMatch) {
					const [T, TResult, TNext] = genericMatch[1].split(",").map((p) => p.trim());
					line.def = line.def + `\nnext(value?: ${TNext}): IteratorResult<${T}, ${TResult}>`;
				}
			}
			s += `${exportDeclare(ambient)} class ${line.name}${extendsDef} {\n${line.def}\n}`;
			if (line.original_name && line.original_name !== line.name) s += `\nexport type ${line.original_name} = ${line.name}`;
			break;
		case TypeDefKind.Fn:
			s += `${exportDeclare(ambient)} ${line.def}`;
			break;
		default: s += line.def;
	}
	return correctStringIdent(s, ident);
}
function exportDeclare(ambient) {
	if (ambient) return "export";
	return "export declare";
}
async function processTypeDef(intermediateTypeFile, constEnum) {
	const exports = [];
	const groupedDefs = preprocessTypeDef(await readIntermediateTypeFile(intermediateTypeFile));
	return {
		dts: sortBy(Array.from(groupedDefs), [([namespace]) => namespace]).map(([namespace, defs]) => {
			if (namespace === TOP_LEVEL_NAMESPACE) return defs.map((def) => {
				switch (def.kind) {
					case TypeDefKind.Const:
					case TypeDefKind.Enum:
					case TypeDefKind.StringEnum:
					case TypeDefKind.Fn:
					case TypeDefKind.Struct:
						exports.push(def.name);
						if (def.original_name && def.original_name !== def.name) exports.push(def.original_name);
						break;
					default: break;
				}
				return prettyPrint(def, constEnum, 0);
			}).join("\n\n");
			else {
				exports.push(namespace);
				let declaration = "";
				declaration += `export declare namespace ${namespace} {\n`;
				for (const def of defs) declaration += prettyPrint(def, constEnum, 2, true) + "\n";
				declaration += "}";
				return declaration;
			}
		}).join("\n\n") + "\n",
		exports
	};
}
async function readIntermediateTypeFile(file) {
	return (await readFileAsync(file, "utf8")).split("\n").filter(Boolean).map((line) => {
		line = line.trim();
		const parsed = JSON.parse(line);
		if (parsed.js_doc) parsed.js_doc = parsed.js_doc.replace(/\\n/g, "\n");
		if (parsed.def) parsed.def = parsed.def.replace(/\\n/g, "\n");
		return parsed;
	}).sort((a, b) => {
		if (a.kind === TypeDefKind.Struct) {
			if (b.kind === TypeDefKind.Struct) return a.name.localeCompare(b.name);
			return -1;
		} else if (b.kind === TypeDefKind.Struct) return 1;
		else return a.name.localeCompare(b.name);
	});
}
function preprocessTypeDef(defs) {
	const namespaceGrouped = /* @__PURE__ */ new Map();
	const classDefs = /* @__PURE__ */ new Map();
	for (const def of defs) {
		const namespace = def.js_mod ?? TOP_LEVEL_NAMESPACE;
		if (!namespaceGrouped.has(namespace)) namespaceGrouped.set(namespace, []);
		const group = namespaceGrouped.get(namespace);
		if (def.kind === TypeDefKind.Struct) {
			group.push(def);
			classDefs.set(def.name, def);
		} else if (def.kind === TypeDefKind.Extends) {
			const classDef = classDefs.get(def.name);
			if (classDef) classDef.extends = def.def;
		} else if (def.kind === TypeDefKind.Impl) {
			const classDef = classDefs.get(def.name);
			if (classDef) {
				if (classDef.def) classDef.def += "\n";
				classDef.def += def.def;
				if (classDef.def) classDef.def = classDef.def.replace(/\\n/g, "\n");
			}
		} else group.push(def);
	}
	return namespaceGrouped;
}
function correctStringIdent(src, ident) {
	let bracketDepth = 0;
	return src.split("\n").map((line) => {
		line = line.trim();
		if (line === "") return "";
		const isInMultilineComment = line.startsWith("*");
		const isClosingBracket = line.endsWith("}");
		const isOpeningBracket = line.endsWith("{");
		const isTypeDeclaration = line.endsWith("=");
		const isTypeVariant = line.startsWith("|");
		let rightIndent = ident;
		if ((isOpeningBracket || isTypeDeclaration) && !isInMultilineComment) {
			bracketDepth += 1;
			rightIndent += (bracketDepth - 1) * 2;
		} else {
			if (isClosingBracket && bracketDepth > 0 && !isInMultilineComment && !isTypeVariant) bracketDepth -= 1;
			rightIndent += bracketDepth * 2;
		}
		if (isInMultilineComment) rightIndent += 1;
		return `${" ".repeat(rightIndent)}${line}`;
	}).join("\n");
}

//#endregion
//#region src/utils/read-config.ts
async function readConfig(options) {
	const resolvePath = (...paths) => resolve(options.cwd, ...paths);
	return await readNapiConfig(resolvePath(options.packageJsonPath ?? "package.json"), options.configPath ? resolvePath(options.configPath) : void 0);
}

//#endregion
//#region src/api/artifacts.ts
const debug$8 = debugFactory("artifacts");
async function collectArtifacts(userOptions) {
	const options = applyDefaultArtifactsOptions(userOptions);
	const resolvePath = (...paths) => resolve(options.cwd, ...paths);
	const packageJsonPath = resolvePath(options.packageJsonPath);
	const { targets, binaryName, packageName } = await readNapiConfig(packageJsonPath, options.configPath ? resolvePath(options.configPath) : void 0);
	const distDirs = targets.map((platform) => join(options.cwd, options.npmDir, platform.platformArchABI));
	const universalSourceBins = new Set(targets.filter((platform) => platform.arch === "universal").flatMap((p) => {
		var _UniArchsByPlatform$p;
		return (_UniArchsByPlatform$p = UniArchsByPlatform[p.platform]) === null || _UniArchsByPlatform$p === void 0 ? void 0 : _UniArchsByPlatform$p.map((a) => `${p.platform}-${a}`);
	}).filter(Boolean));
	await collectNodeBinaries(join(options.cwd, options.outputDir)).then((output) => Promise.all(output.map(async (filePath) => {
		debug$8.info(`Read [${colors.yellowBright(filePath)}]`);
		const sourceContent = await readFileAsync(filePath);
		const parsedName = parse(filePath);
		const terms = parsedName.name.split(".");
		const platformArchABI = terms.pop();
		const _binaryName = terms.join(".");
		if (_binaryName !== binaryName) {
			debug$8.warn(`[${_binaryName}] is not matched with [${binaryName}], skip`);
			return;
		}
		const dir$1 = distDirs.find((dir$2) => dir$2.includes(platformArchABI));
		if (!dir$1 && universalSourceBins.has(platformArchABI)) {
			debug$8.warn(`[${platformArchABI}] has no dist dir but it is source bin for universal arch, skip`);
			return;
		}
		if (!dir$1) throw new Error(`No dist dir found for ${filePath}`);
		const distFilePath = join(dir$1, parsedName.base);
		debug$8.info(`Write file content to [${colors.yellowBright(distFilePath)}]`);
		await writeFileAsync(distFilePath, sourceContent);
		const distFilePathLocal = join(parse(packageJsonPath).dir, parsedName.base);
		debug$8.info(`Write file content to [${colors.yellowBright(distFilePathLocal)}]`);
		await writeFileAsync(distFilePathLocal, sourceContent);
	})));
	const wasiTarget = targets.find((t) => t.platform === "wasi");
	if (wasiTarget) {
		const wasiDir = join(options.cwd, options.npmDir, wasiTarget.platformArchABI);
		const cjsFile = join(options.buildOutputDir ?? options.cwd, `${binaryName}.wasi.cjs`);
		const workerFile = join(options.buildOutputDir ?? options.cwd, `wasi-worker.mjs`);
		const browserEntry = join(options.buildOutputDir ?? options.cwd, `${binaryName}.wasi-browser.js`);
		const browserWorkerFile = join(options.buildOutputDir ?? options.cwd, `wasi-worker-browser.mjs`);
		debug$8.info(`Move wasi binding file [${colors.yellowBright(cjsFile)}] to [${colors.yellowBright(wasiDir)}]`);
		await writeFileAsync(join(wasiDir, `${binaryName}.wasi.cjs`), await readFileAsync(cjsFile));
		debug$8.info(`Move wasi worker file [${colors.yellowBright(workerFile)}] to [${colors.yellowBright(wasiDir)}]`);
		await writeFileAsync(join(wasiDir, `wasi-worker.mjs`), await readFileAsync(workerFile));
		debug$8.info(`Move wasi browser entry file [${colors.yellowBright(browserEntry)}] to [${colors.yellowBright(wasiDir)}]`);
		await writeFileAsync(join(wasiDir, `${binaryName}.wasi-browser.js`), (await readFileAsync(browserEntry, "utf8")).replace(`new URL('./wasi-worker-browser.mjs', import.meta.url)`, `new URL('${packageName}-wasm32-wasi/wasi-worker-browser.mjs', import.meta.url)`));
		debug$8.info(`Move wasi browser worker file [${colors.yellowBright(browserWorkerFile)}] to [${colors.yellowBright(wasiDir)}]`);
		await writeFileAsync(join(wasiDir, `wasi-worker-browser.mjs`), await readFileAsync(browserWorkerFile));
	}
}
async function collectNodeBinaries(root) {
	const files = await readdirAsync(root, { withFileTypes: true });
	const nodeBinaries = files.filter((file) => file.isFile() && (file.name.endsWith(".node") || file.name.endsWith(".wasm"))).map((file) => join(root, file.name));
	const dirs = files.filter((file) => file.isDirectory());
	for (const dir$1 of dirs) if (dir$1.name !== "node_modules") nodeBinaries.push(...await collectNodeBinaries(join(root, dir$1.name)));
	return nodeBinaries;
}

//#endregion
//#region src/api/templates/js-binding.ts
function createCjsBinding(localName, pkgName, idents, packageVersion) {
	return `${bindingHeader}
${createCommonBinding(localName, pkgName, packageVersion)}
module.exports = nativeBinding
${idents.map((ident) => `module.exports.${ident} = nativeBinding.${ident}`).join("\n")}
`;
}
function createEsmBinding(localName, pkgName, idents, packageVersion) {
	return `${bindingHeader}
import { createRequire } from 'node:module'
const require = createRequire(import.meta.url)
const __dirname = new URL('.', import.meta.url).pathname

${createCommonBinding(localName, pkgName, packageVersion)}
const { ${idents.join(", ")} } = nativeBinding
${idents.map((ident) => `export { ${ident} }`).join("\n")}
`;
}
const bindingHeader = `// prettier-ignore
/* eslint-disable */
// @ts-nocheck
/* auto-generated by NAPI-RS */
`;
function createCommonBinding(localName, pkgName, packageVersion) {
	function requireTuple(tuple, identSize = 8) {
		const identLow = " ".repeat(identSize - 2);
		const ident = " ".repeat(identSize);
		return `try {
${ident}return require('./${localName}.${tuple}.node')
${identLow}} catch (e) {
${ident}loadErrors.push(e)
${identLow}}${packageVersion ? `
${identLow}try {
${ident}const binding = require('${pkgName}-${tuple}')
${ident}const bindingPackageVersion = require('${pkgName}-${tuple}/package.json').version
${ident}if (bindingPackageVersion !== '${packageVersion}' && process.env.NAPI_RS_ENFORCE_VERSION_CHECK && process.env.NAPI_RS_ENFORCE_VERSION_CHECK !== '0') {
${ident}  throw new Error(\`Native binding package version mismatch, expected ${packageVersion} but got \${bindingPackageVersion}. You can reinstall dependencies to fix this issue.\`)
${ident}}
${ident}return binding
${identLow}} catch (e) {
${ident}loadErrors.push(e)
${identLow}}` : `
${identLow}try {
${ident}return require('${pkgName}-${tuple}')
${identLow}} catch (e) {
${ident}loadErrors.push(e)
${identLow}}`}`;
	}
	return `const { readFileSync } = require('node:fs')
let nativeBinding = null
const loadErrors = []

const isMusl = () => {
  let musl = false
  if (process.platform === 'linux') {
    musl = isMuslFromFilesystem()
    if (musl === null) {
      musl = isMuslFromReport()
    }
    if (musl === null) {
      musl = isMuslFromChildProcess()
    }
  }
  return musl
}

const isFileMusl = (f) => f.includes('libc.musl-') || f.includes('ld-musl-')

const isMuslFromFilesystem = () => {
  try {
    return readFileSync('/usr/bin/ldd', 'utf-8').includes('musl')
  } catch {
    return null
  }
}

const isMuslFromReport = () => {
  let report = null
  if (typeof process.report?.getReport === 'function') {
    process.report.excludeNetwork = true
    report = process.report.getReport()
  }
  if (!report) {
    return null
  }
  if (report.header && report.header.glibcVersionRuntime) {
    return false
  }
  if (Array.isArray(report.sharedObjects)) {
    if (report.sharedObjects.some(isFileMusl)) {
      return true
    }
  }
  return false
}

const isMuslFromChildProcess = () => {
  try {
    return require('child_process').execSync('ldd --version', { encoding: 'utf8' }).includes('musl')
  } catch (e) {
    // If we reach this case, we don't know if the system is musl or not, so is better to just fallback to false
    return false
  }
}

function requireNative() {
  if (process.env.NAPI_RS_NATIVE_LIBRARY_PATH) {
    try {
      return require(process.env.NAPI_RS_NATIVE_LIBRARY_PATH);
    } catch (err) {
      loadErrors.push(err)
    }
  } else if (process.platform === 'android') {
    if (process.arch === 'arm64') {
      ${requireTuple("android-arm64")}
    } else if (process.arch === 'arm') {
      ${requireTuple("android-arm-eabi")}
    } else {
      loadErrors.push(new Error(\`Unsupported architecture on Android \${process.arch}\`))
    }
  } else if (process.platform === 'win32') {
    if (process.arch === 'x64') {
      if (process.config?.variables?.shlib_suffix === 'dll.a' || process.config?.variables?.node_target_type === 'shared_library') {
        ${requireTuple("win32-x64-gnu")}
      } else {
        ${requireTuple("win32-x64-msvc")}
      }
    } else if (process.arch === 'ia32') {
      ${requireTuple("win32-ia32-msvc")}
    } else if (process.arch === 'arm64') {
      ${requireTuple("win32-arm64-msvc")}
    } else {
      loadErrors.push(new Error(\`Unsupported architecture on Windows: \${process.arch}\`))
    }
  } else if (process.platform === 'darwin') {
    ${requireTuple("darwin-universal", 6)}
    if (process.arch === 'x64') {
      ${requireTuple("darwin-x64")}
    } else if (process.arch === 'arm64') {
      ${requireTuple("darwin-arm64")}
    } else {
      loadErrors.push(new Error(\`Unsupported architecture on macOS: \${process.arch}\`))
    }
  } else if (process.platform === 'freebsd') {
    if (process.arch === 'x64') {
      ${requireTuple("freebsd-x64")}
    } else if (process.arch === 'arm64') {
      ${requireTuple("freebsd-arm64")}
    } else {
      loadErrors.push(new Error(\`Unsupported architecture on FreeBSD: \${process.arch}\`))
    }
  } else if (process.platform === 'linux') {
    if (process.arch === 'x64') {
      if (isMusl()) {
        ${requireTuple("linux-x64-musl", 10)}
      } else {
        ${requireTuple("linux-x64-gnu", 10)}
      }
    } else if (process.arch === 'arm64') {
      if (isMusl()) {
        ${requireTuple("linux-arm64-musl", 10)}
      } else {
        ${requireTuple("linux-arm64-gnu", 10)}
      }
    } else if (process.arch === 'arm') {
      if (isMusl()) {
        ${requireTuple("linux-arm-musleabihf", 10)}
      } else {
        ${requireTuple("linux-arm-gnueabihf", 10)}
      }
    } else if (process.arch === 'loong64') {
      if (isMusl()) {
        ${requireTuple("linux-loong64-musl", 10)}
      } else {
        ${requireTuple("linux-loong64-gnu", 10)}
      }
    } else if (process.arch === 'riscv64') {
      if (isMusl()) {
        ${requireTuple("linux-riscv64-musl", 10)}
      } else {
        ${requireTuple("linux-riscv64-gnu", 10)}
      }
    } else if (process.arch === 'ppc64') {
      ${requireTuple("linux-ppc64-gnu")}
    } else if (process.arch === 's390x') {
      ${requireTuple("linux-s390x-gnu")}
    } else {
      loadErrors.push(new Error(\`Unsupported architecture on Linux: \${process.arch}\`))
    }
  } else if (process.platform === 'openharmony') {
    if (process.arch === 'arm64') {
      ${requireTuple("openharmony-arm64")}
    } else if (process.arch === 'x64') {
      ${requireTuple("openharmony-x64")}
    } else if (process.arch === 'arm') {
      ${requireTuple("openharmony-arm")}
    } else {
      loadErrors.push(new Error(\`Unsupported architecture on OpenHarmony: \${process.arch}\`))
    }
  } else {
    loadErrors.push(new Error(\`Unsupported OS: \${process.platform}, architecture: \${process.arch}\`))
  }
}

nativeBinding = requireNative()

if (!nativeBinding || process.env.NAPI_RS_FORCE_WASI) {
  let wasiBinding = null
  let wasiBindingError = null
  try {
    wasiBinding = require('./${localName}.wasi.cjs')
    nativeBinding = wasiBinding
  } catch (err) {
    if (process.env.NAPI_RS_FORCE_WASI) {
      wasiBindingError = err
    }
  }
  if (!nativeBinding || process.env.NAPI_RS_FORCE_WASI) {
    try {
      wasiBinding = require('${pkgName}-wasm32-wasi')
      nativeBinding = wasiBinding
    } catch (err) {
      if (process.env.NAPI_RS_FORCE_WASI) {
        if (!wasiBindingError) {
          wasiBindingError = err
        } else {
          wasiBindingError.cause = err
        }
        loadErrors.push(err)
      }
    }
  }
  if (process.env.NAPI_RS_FORCE_WASI === 'error' && !wasiBinding) {
    const error = new Error('WASI binding not found and NAPI_RS_FORCE_WASI is set to error')
    error.cause = wasiBindingError
    throw error
  }
}

if (!nativeBinding) {
  if (loadErrors.length > 0) {
    throw new Error(
      \`Cannot find native binding. \` +
        \`npm has a bug related to optional dependencies (https://github.com/npm/cli/issues/4828). \` +
        'Please try \`npm i\` again after removing both package-lock.json and node_modules directory.',
      {
        cause: loadErrors.reduce((err, cur) => {
          cur.cause = err
          return cur
        }),
      },
    )
  }
  throw new Error(\`Failed to load native binding\`)
}
`;
}

//#endregion
//#region src/api/templates/load-wasi-template.ts
const createWasiBrowserBinding = (wasiFilename, initialMemory = 4e3, maximumMemory = 65536, fs$1 = false, asyncInit = false, buffer = false) => {
	return `import {
  createOnMessage as __wasmCreateOnMessageForFsProxy,
  getDefaultContext as __emnapiGetDefaultContext,
  ${asyncInit ? `instantiateNapiModule as __emnapiInstantiateNapiModule` : `instantiateNapiModuleSync as __emnapiInstantiateNapiModuleSync`},
  WASI as __WASI,
} from '@napi-rs/wasm-runtime'
${fs$1 ? buffer ? `import { memfs, Buffer } from '@napi-rs/wasm-runtime/fs'` : `import { memfs } from '@napi-rs/wasm-runtime/fs'` : ""}
${buffer && !fs$1 ? `import { Buffer } from 'buffer'` : ""}
${fs$1 ? `
export const { fs: __fs, vol: __volume } = memfs()

const __wasi = new __WASI({
  version: 'preview1',
  fs: __fs,
  preopens: {
    '/': '/',
  },
})` : `
const __wasi = new __WASI({
  version: 'preview1',
})`}

const __wasmUrl = new URL('./${wasiFilename}.wasm', import.meta.url).href
const __emnapiContext = __emnapiGetDefaultContext()
${buffer ? "__emnapiContext.feature.Buffer = Buffer" : ""}

const __sharedMemory = new WebAssembly.Memory({
  initial: ${initialMemory},
  maximum: ${maximumMemory},
  shared: true,
})

const __wasmFile = await fetch(__wasmUrl).then((res) => res.arrayBuffer())

const {
  instance: __napiInstance,
  module: __wasiModule,
  napiModule: __napiModule,
} = ${asyncInit ? `await __emnapiInstantiateNapiModule` : `__emnapiInstantiateNapiModuleSync`}(__wasmFile, {
  context: __emnapiContext,
  asyncWorkPoolSize: 4,
  wasi: __wasi,
  onCreateWorker() {
    const worker = new Worker(new URL('./wasi-worker-browser.mjs', import.meta.url), {
      type: 'module',
    })
${fs$1 ? `    worker.addEventListener('message', __wasmCreateOnMessageForFsProxy(__fs))\n` : ""}
    return worker
  },
  overwriteImports(importObject) {
    importObject.env = {
      ...importObject.env,
      ...importObject.napi,
      ...importObject.emnapi,
      memory: __sharedMemory,
    }
    return importObject
  },
  beforeInit({ instance }) {
    for (const name of Object.keys(instance.exports)) {
      if (name.startsWith('__napi_register__')) {
        instance.exports[name]()
      }
    }
  },
})
`;
};
const createWasiBinding = (wasmFileName, packageName, initialMemory = 4e3, maximumMemory = 65536) => `/* eslint-disable */
/* prettier-ignore */

/* auto-generated by NAPI-RS */

const __nodeFs = require('node:fs')
const __nodePath = require('node:path')
const { WASI: __nodeWASI } = require('node:wasi')
const { Worker } = require('node:worker_threads')

const {
  createOnMessage: __wasmCreateOnMessageForFsProxy,
  getDefaultContext: __emnapiGetDefaultContext,
  instantiateNapiModuleSync: __emnapiInstantiateNapiModuleSync,
} = require('@napi-rs/wasm-runtime')

const __rootDir = __nodePath.parse(process.cwd()).root

const __wasi = new __nodeWASI({
  version: 'preview1',
  env: process.env,
  preopens: {
    [__rootDir]: __rootDir,
  }
})

const __emnapiContext = __emnapiGetDefaultContext()

const __sharedMemory = new WebAssembly.Memory({
  initial: ${initialMemory},
  maximum: ${maximumMemory},
  shared: true,
})

let __wasmFilePath = __nodePath.join(__dirname, '${wasmFileName}.wasm')
const __wasmDebugFilePath = __nodePath.join(__dirname, '${wasmFileName}.debug.wasm')

if (__nodeFs.existsSync(__wasmDebugFilePath)) {
  __wasmFilePath = __wasmDebugFilePath
} else if (!__nodeFs.existsSync(__wasmFilePath)) {
  try {
    __wasmFilePath = require.resolve('${packageName}-wasm32-wasi/${wasmFileName}.wasm')
  } catch {
    throw new Error('Cannot find ${wasmFileName}.wasm file, and ${packageName}-wasm32-wasi package is not installed.')
  }
}

const { instance: __napiInstance, module: __wasiModule, napiModule: __napiModule } = __emnapiInstantiateNapiModuleSync(__nodeFs.readFileSync(__wasmFilePath), {
  context: __emnapiContext,
  asyncWorkPoolSize: (function() {
    const threadsSizeFromEnv = Number(process.env.NAPI_RS_ASYNC_WORK_POOL_SIZE ?? process.env.UV_THREADPOOL_SIZE)
    // NaN > 0 is false
    if (threadsSizeFromEnv > 0) {
      return threadsSizeFromEnv
    } else {
      return 4
    }
  })(),
  reuseWorker: true,
  wasi: __wasi,
  onCreateWorker() {
    const worker = new Worker(__nodePath.join(__dirname, 'wasi-worker.mjs'), {
      env: process.env,
    })
    worker.onmessage = ({ data }) => {
      __wasmCreateOnMessageForFsProxy(__nodeFs)(data)
    }

    // The main thread of Node.js waits for all the active handles before exiting.
    // But Rust threads are never waited without \`thread::join\`.
    // So here we hack the code of Node.js to prevent the workers from being referenced (active).
    // According to https://github.com/nodejs/node/blob/19e0d472728c79d418b74bddff588bea70a403d0/lib/internal/worker.js#L415,
    // a worker is consist of two handles: kPublicPort and kHandle.
    {
      const kPublicPort = Object.getOwnPropertySymbols(worker).find(s =>
        s.toString().includes("kPublicPort")
      );
      if (kPublicPort) {
        worker[kPublicPort].ref = () => {};
      }

      const kHandle = Object.getOwnPropertySymbols(worker).find(s =>
        s.toString().includes("kHandle")
      );
      if (kHandle) {
        worker[kHandle].ref = () => {};
      }

      worker.unref();
    }
    return worker
  },
  overwriteImports(importObject) {
    importObject.env = {
      ...importObject.env,
      ...importObject.napi,
      ...importObject.emnapi,
      memory: __sharedMemory,
    }
    return importObject
  },
  beforeInit({ instance }) {
    for (const name of Object.keys(instance.exports)) {
      if (name.startsWith('__napi_register__')) {
        instance.exports[name]()
      }
    }
  },
})
`;

//#endregion
//#region src/api/templates/wasi-worker-template.ts
const WASI_WORKER_TEMPLATE = `import fs from "node:fs";
import { createRequire } from "node:module";
import { parse } from "node:path";
import { WASI } from "node:wasi";
import { parentPort, Worker } from "node:worker_threads";

const require = createRequire(import.meta.url);

const { instantiateNapiModuleSync, MessageHandler, getDefaultContext } = require("@napi-rs/wasm-runtime");

if (parentPort) {
  parentPort.on("message", (data) => {
    globalThis.onmessage({ data });
  });
}

Object.assign(globalThis, {
  self: globalThis,
  require,
  Worker,
  importScripts: function (f) {
    ;(0, eval)(fs.readFileSync(f, "utf8") + "//# sourceURL=" + f);
  },
  postMessage: function (msg) {
    if (parentPort) {
      parentPort.postMessage(msg);
    }
  },
});

const emnapiContext = getDefaultContext();

const __rootDir = parse(process.cwd()).root;

const handler = new MessageHandler({
  onLoad({ wasmModule, wasmMemory }) {
    const wasi = new WASI({
      version: 'preview1',
      env: process.env,
      preopens: {
        [__rootDir]: __rootDir,
      },
    });

    return instantiateNapiModuleSync(wasmModule, {
      childThread: true,
      wasi,
      context: emnapiContext,
      overwriteImports(importObject) {
        importObject.env = {
          ...importObject.env,
          ...importObject.napi,
          ...importObject.emnapi,
          memory: wasmMemory
        };
      },
    });
  },
});

globalThis.onmessage = function (e) {
  handler.handle(e);
};
`;
const createWasiBrowserWorkerBinding = (fs$1) => {
	return `${fs$1 ? `import { instantiateNapiModuleSync, MessageHandler, WASI, createFsProxy } from '@napi-rs/wasm-runtime'
import { memfsExported as __memfsExported } from '@napi-rs/wasm-runtime/fs'

const fs = createFsProxy(__memfsExported)` : `import { instantiateNapiModuleSync, MessageHandler, WASI } from '@napi-rs/wasm-runtime'`}

const handler = new MessageHandler({
  onLoad({ wasmModule, wasmMemory }) {
    ${fs$1 ? `const wasi = new WASI({
      fs,
      preopens: {
        '/': '/',
      },
      print: function () {
        // eslint-disable-next-line no-console
        console.log.apply(console, arguments)
      },
      printErr: function() {
        // eslint-disable-next-line no-console
        console.error.apply(console, arguments)
      },
    })` : `const wasi = new WASI({
      print: function () {
        // eslint-disable-next-line no-console
        console.log.apply(console, arguments)
      },
      printErr: function() {
        // eslint-disable-next-line no-console
        console.error.apply(console, arguments)
      },
    })`}
    return instantiateNapiModuleSync(wasmModule, {
      childThread: true,
      wasi,
      overwriteImports(importObject) {
        importObject.env = {
          ...importObject.env,
          ...importObject.napi,
          ...importObject.emnapi,
          memory: wasmMemory,
        }
      },
    })
  },
})

globalThis.onmessage = function (e) {
  handler.handle(e)
}
`;
};

//#endregion
//#region src/api/build.ts
const debug$7 = debugFactory("build");
const require = createRequire(import.meta.url);
async function buildProject(rawOptions) {
	debug$7("napi build command receive options: %O", rawOptions);
	const options = {
		dtsCache: true,
		...rawOptions,
		cwd: rawOptions.cwd ?? process.cwd()
	};
	const resolvePath = (...paths) => resolve(options.cwd, ...paths);
	const manifestPath = resolvePath(options.manifestPath ?? "Cargo.toml");
	const metadata = await parseMetadata(manifestPath);
	const crate = metadata.packages.find((p) => {
		if (options.package) return p.name === options.package;
		else return p.manifest_path === manifestPath;
	});
	if (!crate) throw new Error("Unable to find crate to build. It seems you are trying to build a crate in a workspace, try using `--package` option to specify the package to build.");
	return new Builder(metadata, crate, await readNapiConfig(resolvePath(options.packageJsonPath ?? "package.json"), options.configPath ? resolvePath(options.configPath) : void 0), options).build();
}
var Builder = class {
	args = [];
	envs = {};
	outputs = [];
	target;
	crateDir;
	outputDir;
	targetDir;
	enableTypeDef = false;
	constructor(metadata, crate, config, options) {
		this.metadata = metadata;
		this.crate = crate;
		this.config = config;
		this.options = options;
		this.target = options.target ? parseTriple(options.target) : process.env.CARGO_BUILD_TARGET ? parseTriple(process.env.CARGO_BUILD_TARGET) : getSystemDefaultTarget();
		this.crateDir = parse(crate.manifest_path).dir;
		this.outputDir = resolve(this.options.cwd, options.outputDir ?? this.crateDir);
		this.targetDir = options.targetDir ?? process.env.CARGO_BUILD_TARGET_DIR ?? metadata.target_directory;
		this.enableTypeDef = this.crate.dependencies.some((dep) => dep.name === "napi-derive" && (dep.uses_default_features || dep.features.includes("type-def")));
		if (!this.enableTypeDef) {
			const requirementWarning = "`napi-derive` crate is not used or `type-def` feature is not enabled for `napi-derive` crate";
			debug$7.warn(`${requirementWarning}. Will skip binding generation for \`.node\`, \`.wasi\` and \`.d.ts\` files.`);
			if (this.options.dts || this.options.dtsHeader || this.config.dtsHeader || this.config.dtsHeaderFile) debug$7.warn(`${requirementWarning}. \`dts\` related options are enabled but will be ignored.`);
		}
	}
	get cdyLibName() {
		var _this$crate$targets$f;
		return (_this$crate$targets$f = this.crate.targets.find((t) => t.crate_types.includes("cdylib"))) === null || _this$crate$targets$f === void 0 ? void 0 : _this$crate$targets$f.name;
	}
	get binName() {
		var _this$crate$targets$f2;
		return this.options.bin ?? (this.cdyLibName ? null : (_this$crate$targets$f2 = this.crate.targets.find((t) => t.crate_types.includes("bin"))) === null || _this$crate$targets$f2 === void 0 ? void 0 : _this$crate$targets$f2.name);
	}
	build() {
		if (!this.cdyLibName) {
			const warning = "Missing `crate-type = [\"cdylib\"]` in [lib] config. The build result will not be available as node addon.";
			if (this.binName) debug$7.warn(warning);
			else throw new Error(warning);
		}
		return this.pickBinary().setPackage().setFeatures().setTarget().pickCrossToolchain().setEnvs().setBypassArgs().exec();
	}
	pickCrossToolchain() {
		if (!this.options.useNapiCross) return this;
		if (this.options.useCross) debug$7.warn("You are trying to use both `--cross` and `--use-napi-cross` options, `--use-cross` will be ignored.");
		if (this.options.crossCompile) debug$7.warn("You are trying to use both `--cross-compile` and `--use-napi-cross` options, `--cross-compile` will be ignored.");
		try {
			var _process$env$TARGET_C, _process$env$CC, _process$env$CXX, _process$env$TARGET_C2;
			const { version: version$2, download } = require("@napi-rs/cross-toolchain");
			const alias = { "s390x-unknown-linux-gnu": "s390x-ibm-linux-gnu" };
			const toolchainPath = join(homedir(), ".napi-rs", "cross-toolchain", version$2, this.target.triple);
			mkdirSync(toolchainPath, { recursive: true });
			if (existsSync(join(toolchainPath, "package.json"))) debug$7(`Toolchain ${toolchainPath} exists, skip extracting`);
			else download(process.arch, this.target.triple).unpack(toolchainPath);
			const upperCaseTarget = targetToEnvVar(this.target.triple);
			const crossTargetName = alias[this.target.triple] ?? this.target.triple;
			const linkerEnv = `CARGO_TARGET_${upperCaseTarget}_LINKER`;
			this.setEnvIfNotExists(linkerEnv, join(toolchainPath, "bin", `${crossTargetName}-gcc`));
			this.setEnvIfNotExists("TARGET_SYSROOT", join(toolchainPath, crossTargetName, "sysroot"));
			this.setEnvIfNotExists("TARGET_AR", join(toolchainPath, "bin", `${crossTargetName}-ar`));
			this.setEnvIfNotExists("TARGET_RANLIB", join(toolchainPath, "bin", `${crossTargetName}-ranlib`));
			this.setEnvIfNotExists("TARGET_READELF", join(toolchainPath, "bin", `${crossTargetName}-readelf`));
			this.setEnvIfNotExists("TARGET_C_INCLUDE_PATH", join(toolchainPath, crossTargetName, "sysroot", "usr", "include/"));
			this.setEnvIfNotExists("TARGET_CC", join(toolchainPath, "bin", `${crossTargetName}-gcc`));
			this.setEnvIfNotExists("TARGET_CXX", join(toolchainPath, "bin", `${crossTargetName}-g++`));
			this.setEnvIfNotExists("BINDGEN_EXTRA_CLANG_ARGS", `--sysroot=${this.envs.TARGET_SYSROOT}}`);
			if (((_process$env$TARGET_C = process.env.TARGET_CC) === null || _process$env$TARGET_C === void 0 ? void 0 : _process$env$TARGET_C.startsWith("clang")) || ((_process$env$CC = process.env.CC) === null || _process$env$CC === void 0 ? void 0 : _process$env$CC.startsWith("clang")) && !process.env.TARGET_CC) {
				const TARGET_CFLAGS = process.env.TARGET_CFLAGS ?? "";
				this.envs.TARGET_CFLAGS = `--sysroot=${this.envs.TARGET_SYSROOT} --gcc-toolchain=${toolchainPath} ${TARGET_CFLAGS}`;
			}
			if (((_process$env$CXX = process.env.CXX) === null || _process$env$CXX === void 0 ? void 0 : _process$env$CXX.startsWith("clang++")) && !process.env.TARGET_CXX || ((_process$env$TARGET_C2 = process.env.TARGET_CXX) === null || _process$env$TARGET_C2 === void 0 ? void 0 : _process$env$TARGET_C2.startsWith("clang++"))) {
				const TARGET_CXXFLAGS = process.env.TARGET_CXXFLAGS ?? "";
				this.envs.TARGET_CXXFLAGS = `--sysroot=${this.envs.TARGET_SYSROOT} --gcc-toolchain=${toolchainPath} ${TARGET_CXXFLAGS}`;
			}
			this.envs.PATH = this.envs.PATH ? `${toolchainPath}/bin:${this.envs.PATH}:${process.env.PATH}` : `${toolchainPath}/bin:${process.env.PATH}`;
		} catch (e) {
			debug$7.warn("Pick cross toolchain failed", e);
		}
		return this;
	}
	exec() {
		debug$7(`Start building crate: ${this.crate.name}`);
		debug$7("  %i", `cargo ${this.args.join(" ")}`);
		const controller = new AbortController();
		const watch = this.options.watch;
		return {
			task: new Promise((resolve$1, reject) => {
				var _buildProcess$stderr;
				if (this.options.useCross && this.options.crossCompile) throw new Error("`--use-cross` and `--cross-compile` can not be used together");
				const buildProcess = spawn(process.env.CARGO ?? (this.options.useCross ? "cross" : "cargo"), this.args, {
					env: {
						...process.env,
						...this.envs
					},
					stdio: watch ? [
						"inherit",
						"inherit",
						"pipe"
					] : "inherit",
					cwd: this.options.cwd,
					signal: controller.signal
				});
				buildProcess.once("exit", (code) => {
					if (code === 0) {
						debug$7("%i", `Build crate ${this.crate.name} successfully!`);
						resolve$1();
					} else reject(/* @__PURE__ */ new Error(`Build failed with exit code ${code}`));
				});
				buildProcess.once("error", (e) => {
					reject(new Error(`Build failed with error: ${e.message}`, { cause: e }));
				});
				(_buildProcess$stderr = buildProcess.stderr) === null || _buildProcess$stderr === void 0 || _buildProcess$stderr.on("data", (data) => {
					const output = data.toString();
					console.error(output);
					if (/Finished\s(`dev`|`release`)/.test(output)) this.postBuild().catch(() => {});
				});
			}).then(() => this.postBuild()),
			abort: () => controller.abort()
		};
	}
	pickBinary() {
		let set = false;
		if (this.options.watch) if (process.env.CI) debug$7.warn("Watch mode is not supported in CI environment");
		else {
			debug$7("Use %i", "cargo-watch");
			tryInstallCargoBinary("cargo-watch", "watch");
			this.args.push("watch", "--why", "-i", "*.{js,ts,node}", "-w", this.crateDir, "--", "cargo", "build");
			set = true;
		}
		if (this.options.crossCompile) if (this.target.platform === "win32") if (process.platform === "win32") debug$7.warn("You are trying to cross compile to win32 platform on win32 platform which is unnecessary.");
		else {
			debug$7("Use %i", "cargo-xwin");
			tryInstallCargoBinary("cargo-xwin", "xwin");
			this.args.push("xwin", "build");
			if (this.target.arch === "ia32") this.envs.XWIN_ARCH = "x86";
			set = true;
		}
		else if (this.target.platform === "linux" && process.platform === "linux" && this.target.arch === process.arch && (function(abi) {
			var _process$report;
			return abi === (((_process$report = process.report) === null || _process$report === void 0 || (_process$report = _process$report.getReport()) === null || _process$report === void 0 || (_process$report = _process$report.header) === null || _process$report === void 0 ? void 0 : _process$report.glibcVersionRuntime) ? "gnu" : "musl");
		})(this.target.abi)) debug$7.warn("You are trying to cross compile to linux target on linux platform which is unnecessary.");
		else if (this.target.platform === "darwin" && process.platform === "darwin") debug$7.warn("You are trying to cross compile to darwin target on darwin platform which is unnecessary.");
		else {
			debug$7("Use %i", "cargo-zigbuild");
			tryInstallCargoBinary("cargo-zigbuild", "zigbuild");
			this.args.push("zigbuild");
			set = true;
		}
		if (!set) this.args.push("build");
		return this;
	}
	setPackage() {
		const args = [];
		if (this.options.package) args.push("--package", this.options.package);
		if (this.binName) args.push("--bin", this.binName);
		if (args.length) {
			debug$7("Set package flags: ");
			debug$7("  %O", args);
			this.args.push(...args);
		}
		return this;
	}
	setTarget() {
		debug$7("Set compiling target to: ");
		debug$7("  %i", this.target.triple);
		this.args.push("--target", this.target.triple);
		return this;
	}
	setEnvs() {
		var _this$target$abi;
		if (this.enableTypeDef) {
			this.envs.NAPI_TYPE_DEF_TMP_FOLDER = this.generateIntermediateTypeDefFolder();
			this.setForceBuildEnvs(this.envs.NAPI_TYPE_DEF_TMP_FOLDER);
		}
		let rustflags = process.env.RUSTFLAGS ?? process.env.CARGO_BUILD_RUSTFLAGS ?? "";
		if (((_this$target$abi = this.target.abi) === null || _this$target$abi === void 0 ? void 0 : _this$target$abi.includes("musl")) && !rustflags.includes("target-feature=-crt-static")) rustflags += " -C target-feature=-crt-static";
		if (this.options.strip && !rustflags.includes("link-arg=-s")) rustflags += " -C link-arg=-s";
		if (rustflags.length) this.envs.RUSTFLAGS = rustflags;
		const linker = this.options.crossCompile ? void 0 : getTargetLinker(this.target.triple);
		const linkerEnv = `CARGO_TARGET_${targetToEnvVar(this.target.triple)}_LINKER`;
		if (linker && !process.env[linkerEnv] && !this.envs[linkerEnv]) this.envs[linkerEnv] = linker;
		if (this.target.platform === "android") this.setAndroidEnv();
		if (this.target.platform === "wasi") this.setWasiEnv();
		if (this.target.platform === "openharmony") this.setOpenHarmonyEnv();
		debug$7("Set envs: ");
		Object.entries(this.envs).forEach(([k, v]) => {
			debug$7("  %i", `${k}=${v}`);
		});
		return this;
	}
	setForceBuildEnvs(typeDefTmpFolder) {
		this.metadata.packages.forEach((crate) => {
			if (crate.dependencies.some((d) => d.name === "napi-derive") && !existsSync(join(typeDefTmpFolder, crate.name))) this.envs[`NAPI_FORCE_BUILD_${crate.name.replace(/-/g, "_").toUpperCase()}`] = Date.now().toString();
		});
	}
	setAndroidEnv() {
		const { ANDROID_NDK_LATEST_HOME } = process.env;
		if (!ANDROID_NDK_LATEST_HOME) debug$7.warn(`${colors.red("ANDROID_NDK_LATEST_HOME")} environment variable is missing`);
		if (process.platform === "android") return;
		const targetArch = this.target.arch === "arm" ? "armv7a" : "aarch64";
		const targetPlatform = this.target.arch === "arm" ? "androideabi24" : "android24";
		const hostPlatform = process.platform === "darwin" ? "darwin" : process.platform === "win32" ? "windows" : "linux";
		Object.assign(this.envs, {
			CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER: `${ANDROID_NDK_LATEST_HOME}/toolchains/llvm/prebuilt/${hostPlatform}-x86_64/bin/${targetArch}-linux-android24-clang`,
			CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER: `${ANDROID_NDK_LATEST_HOME}/toolchains/llvm/prebuilt/${hostPlatform}-x86_64/bin/${targetArch}-linux-androideabi24-clang`,
			TARGET_CC: `${ANDROID_NDK_LATEST_HOME}/toolchains/llvm/prebuilt/${hostPlatform}-x86_64/bin/${targetArch}-linux-${targetPlatform}-clang`,
			TARGET_CXX: `${ANDROID_NDK_LATEST_HOME}/toolchains/llvm/prebuilt/${hostPlatform}-x86_64/bin/${targetArch}-linux-${targetPlatform}-clang++`,
			TARGET_AR: `${ANDROID_NDK_LATEST_HOME}/toolchains/llvm/prebuilt/${hostPlatform}-x86_64/bin/llvm-ar`,
			TARGET_RANLIB: `${ANDROID_NDK_LATEST_HOME}/toolchains/llvm/prebuilt/${hostPlatform}-x86_64/bin/llvm-ranlib`,
			ANDROID_NDK: ANDROID_NDK_LATEST_HOME,
			PATH: `${ANDROID_NDK_LATEST_HOME}/toolchains/llvm/prebuilt/${hostPlatform}-x86_64/bin${process.platform === "win32" ? ";" : ":"}${process.env.PATH}`
		});
	}
	setWasiEnv() {
		const emnapi = join(require.resolve("emnapi"), "..", "lib", "wasm32-wasi-threads");
		this.envs.EMNAPI_LINK_DIR = emnapi;
		const { WASI_SDK_PATH } = process.env;
		if (WASI_SDK_PATH && existsSync(WASI_SDK_PATH)) {
			this.envs.CARGO_TARGET_WASM32_WASI_PREVIEW1_THREADS_LINKER = join(WASI_SDK_PATH, "bin", "wasm-ld");
			this.envs.CARGO_TARGET_WASM32_WASIP1_LINKER = join(WASI_SDK_PATH, "bin", "wasm-ld");
			this.envs.CARGO_TARGET_WASM32_WASIP1_THREADS_LINKER = join(WASI_SDK_PATH, "bin", "wasm-ld");
			this.envs.CARGO_TARGET_WASM32_WASIP2_LINKER = join(WASI_SDK_PATH, "bin", "wasm-ld");
			this.setEnvIfNotExists("TARGET_CC", join(WASI_SDK_PATH, "bin", "clang"));
			this.setEnvIfNotExists("TARGET_CXX", join(WASI_SDK_PATH, "bin", "clang++"));
			this.setEnvIfNotExists("TARGET_AR", join(WASI_SDK_PATH, "bin", "ar"));
			this.setEnvIfNotExists("TARGET_RANLIB", join(WASI_SDK_PATH, "bin", "ranlib"));
			this.setEnvIfNotExists("TARGET_CFLAGS", `--target=wasm32-wasi-threads --sysroot=${WASI_SDK_PATH}/share/wasi-sysroot -pthread -mllvm -wasm-enable-sjlj`);
			this.setEnvIfNotExists("TARGET_CXXFLAGS", `--target=wasm32-wasi-threads --sysroot=${WASI_SDK_PATH}/share/wasi-sysroot -pthread -mllvm -wasm-enable-sjlj`);
			this.setEnvIfNotExists(`TARGET_LDFLAGS`, `-fuse-ld=${WASI_SDK_PATH}/bin/wasm-ld --target=wasm32-wasi-threads`);
		}
	}
	setOpenHarmonyEnv() {
		const { OHOS_SDK_PATH, OHOS_SDK_NATIVE } = process.env;
		const ndkPath = OHOS_SDK_PATH ? `${OHOS_SDK_PATH}/native` : OHOS_SDK_NATIVE;
		if (!ndkPath && process.platform !== "openharmony") {
			debug$7.warn(`${colors.red("OHOS_SDK_PATH")} or ${colors.red("OHOS_SDK_NATIVE")} environment variable is missing`);
			return;
		}
		const linkerName = `CARGO_TARGET_${this.target.triple.toUpperCase().replace(/-/g, "_")}_LINKER`;
		const ranPath = `${ndkPath}/llvm/bin/llvm-ranlib`;
		const arPath = `${ndkPath}/llvm/bin/llvm-ar`;
		const ccPath = `${ndkPath}/llvm/bin/${this.target.triple}-clang`;
		const cxxPath = `${ndkPath}/llvm/bin/${this.target.triple}-clang++`;
		const asPath = `${ndkPath}/llvm/bin/llvm-as`;
		const ldPath = `${ndkPath}/llvm/bin/ld.lld`;
		const stripPath = `${ndkPath}/llvm/bin/llvm-strip`;
		const objDumpPath = `${ndkPath}/llvm/bin/llvm-objdump`;
		const objCopyPath = `${ndkPath}/llvm/bin/llvm-objcopy`;
		const nmPath = `${ndkPath}/llvm/bin/llvm-nm`;
		const binPath = `${ndkPath}/llvm/bin`;
		const libPath = `${ndkPath}/llvm/lib`;
		this.setEnvIfNotExists("LIBCLANG_PATH", libPath);
		this.setEnvIfNotExists("DEP_ATOMIC", "clang_rt.builtins");
		this.setEnvIfNotExists(linkerName, ccPath);
		this.setEnvIfNotExists("TARGET_CC", ccPath);
		this.setEnvIfNotExists("TARGET_CXX", cxxPath);
		this.setEnvIfNotExists("TARGET_AR", arPath);
		this.setEnvIfNotExists("TARGET_RANLIB", ranPath);
		this.setEnvIfNotExists("TARGET_AS", asPath);
		this.setEnvIfNotExists("TARGET_LD", ldPath);
		this.setEnvIfNotExists("TARGET_STRIP", stripPath);
		this.setEnvIfNotExists("TARGET_OBJDUMP", objDumpPath);
		this.setEnvIfNotExists("TARGET_OBJCOPY", objCopyPath);
		this.setEnvIfNotExists("TARGET_NM", nmPath);
		this.envs.PATH = `${binPath}${process.platform === "win32" ? ";" : ":"}${process.env.PATH}`;
	}
	setFeatures() {
		const args = [];
		if (this.options.allFeatures && this.options.noDefaultFeatures) throw new Error("Cannot specify --all-features and --no-default-features together");
		if (this.options.allFeatures) args.push("--all-features");
		else if (this.options.noDefaultFeatures) args.push("--no-default-features");
		if (this.options.features) args.push("--features", ...this.options.features);
		debug$7("Set features flags: ");
		debug$7("  %O", args);
		this.args.push(...args);
		return this;
	}
	setBypassArgs() {
		var _this$options$cargoOp;
		if (this.options.release) this.args.push("--release");
		if (this.options.verbose) this.args.push("--verbose");
		if (this.options.targetDir) this.args.push("--target-dir", this.options.targetDir);
		if (this.options.profile) this.args.push("--profile", this.options.profile);
		if (this.options.manifestPath) this.args.push("--manifest-path", this.options.manifestPath);
		if ((_this$options$cargoOp = this.options.cargoOptions) === null || _this$options$cargoOp === void 0 ? void 0 : _this$options$cargoOp.length) this.args.push(...this.options.cargoOptions);
		return this;
	}
	generateIntermediateTypeDefFolder() {
		let folder = join(this.targetDir, "napi-rs", `${this.crate.name}-${createHash("sha256").update(this.crate.manifest_path).update(CLI_VERSION).digest("hex").substring(0, 8)}`);
		if (!this.options.dtsCache) {
			rmSync(folder, {
				recursive: true,
				force: true
			});
			folder += `_${Date.now()}`;
		}
		mkdirAsync(folder, { recursive: true });
		return folder;
	}
	async postBuild() {
		try {
			debug$7(`Try to create output directory:`);
			debug$7("  %i", this.outputDir);
			await mkdirAsync(this.outputDir, { recursive: true });
			debug$7(`Output directory created`);
		} catch (e) {
			throw new Error(`Failed to create output directory ${this.outputDir}`, { cause: e });
		}
		const wasmBinaryName = await this.copyArtifact();
		if (this.cdyLibName) {
			const idents = await this.generateTypeDef();
			const jsOutput = await this.writeJsBinding(idents);
			const wasmBindingsOutput = await this.writeWasiBinding(wasmBinaryName, idents);
			if (jsOutput) this.outputs.push(jsOutput);
			if (wasmBindingsOutput) this.outputs.push(...wasmBindingsOutput);
		}
		return this.outputs;
	}
	async copyArtifact() {
		const [srcName, destName, wasmBinaryName] = this.getArtifactNames();
		if (!srcName || !destName) return;
		const profile = this.options.profile ?? (this.options.release ? "release" : "debug");
		const src = join(this.targetDir, this.target.triple, profile, srcName);
		debug$7(`Copy artifact from: [${src}]`);
		const dest = join(this.outputDir, destName);
		const isWasm = dest.endsWith(".wasm");
		try {
			if (await fileExists(dest)) {
				debug$7("Old artifact found, remove it first");
				await unlinkAsync(dest);
			}
			debug$7("Copy artifact to:");
			debug$7("  %i", dest);
			if (isWasm) {
				const { ModuleConfig } = await import("@napi-rs/wasm-tools");
				debug$7("Generate debug wasm module");
				try {
					const debugWasmBinary = new ModuleConfig().generateDwarf(true).generateNameSection(true).generateProducersSection(true).preserveCodeTransform(true).strictValidate(false).parse(await readFileAsync(src)).emitWasm(true);
					await writeFileAsync(dest.replace(/\.wasm$/, ".debug.wasm"), debugWasmBinary);
					debug$7("Generate release wasm module");
					await writeFileAsync(dest, new ModuleConfig().generateDwarf(false).generateNameSection(false).generateProducersSection(false).preserveCodeTransform(false).strictValidate(false).onlyStableFeatures(false).parse(debugWasmBinary).emitWasm(false));
				} catch (e) {
					debug$7.warn(`Failed to generate debug wasm module: ${e.message ?? e}`);
					await copyFileAsync(src, dest);
				}
			} else await copyFileAsync(src, dest);
			this.outputs.push({
				kind: dest.endsWith(".node") ? "node" : isWasm ? "wasm" : "exe",
				path: dest
			});
			return wasmBinaryName ? join(this.outputDir, wasmBinaryName) : null;
		} catch (e) {
			throw new Error("Failed to copy artifact", { cause: e });
		}
	}
	getArtifactNames() {
		if (this.cdyLibName) {
			const cdyLib = this.cdyLibName.replace(/-/g, "_");
			const wasiTarget = this.config.targets.find((t) => t.platform === "wasi");
			const srcName = this.target.platform === "darwin" ? `lib${cdyLib}.dylib` : this.target.platform === "win32" ? `${cdyLib}.dll` : this.target.platform === "wasi" || this.target.platform === "wasm" ? `${cdyLib}.wasm` : `lib${cdyLib}.so`;
			let destName = this.config.binaryName;
			if (this.options.platform) destName += `.${this.target.platformArchABI}`;
			if (srcName.endsWith(".wasm")) destName += ".wasm";
			else destName += ".node";
			return [
				srcName,
				destName,
				wasiTarget ? `${this.config.binaryName}.${wasiTarget.platformArchABI}.wasm` : null
			];
		} else if (this.binName) {
			const srcName = this.target.platform === "win32" ? `${this.binName}.exe` : this.binName;
			return [srcName, srcName];
		}
		return [];
	}
	async generateTypeDef() {
		const typeDefDir = this.envs.NAPI_TYPE_DEF_TMP_FOLDER;
		if (!this.enableTypeDef) return [];
		const { exports, dts } = await generateTypeDef({
			typeDefDir,
			noDtsHeader: this.options.noDtsHeader,
			dtsHeader: this.options.dtsHeader,
			configDtsHeader: this.config.dtsHeader,
			configDtsHeaderFile: this.config.dtsHeaderFile,
			constEnum: this.options.constEnum ?? this.config.constEnum,
			cwd: this.options.cwd
		});
		const dest = join(this.outputDir, this.options.dts ?? "index.d.ts");
		try {
			debug$7("Writing type def to:");
			debug$7("  %i", dest);
			await writeFileAsync(dest, dts, "utf-8");
		} catch (e) {
			debug$7.error("Failed to write type def file");
			debug$7.error(e);
		}
		if (exports.length > 0) {
			const dest$1 = join(this.outputDir, this.options.dts ?? "index.d.ts");
			this.outputs.push({
				kind: "dts",
				path: dest$1
			});
		}
		return exports;
	}
	async writeJsBinding(idents) {
		return writeJsBinding({
			platform: this.options.platform,
			noJsBinding: this.options.noJsBinding,
			idents,
			jsBinding: this.options.jsBinding,
			esm: this.options.esm,
			binaryName: this.config.binaryName,
			packageName: this.options.jsPackageName ?? this.config.packageName,
			version: process.env.npm_new_version ?? this.config.packageJson.version,
			outputDir: this.outputDir
		});
	}
	async writeWasiBinding(distFileName, idents) {
		if (distFileName) {
			var _this$config$wasm, _this$config$wasm2, _this$config$wasm3, _this$config$wasm4, _this$config$wasm5, _this$config$wasm6, _this$config$wasm7, _this$config$wasm8;
			const { name, dir: dir$1 } = parse(distFileName);
			const bindingPath = join(dir$1, `${this.config.binaryName}.wasi.cjs`);
			const browserBindingPath = join(dir$1, `${this.config.binaryName}.wasi-browser.js`);
			const workerPath = join(dir$1, "wasi-worker.mjs");
			const browserWorkerPath = join(dir$1, "wasi-worker-browser.mjs");
			const browserEntryPath = join(dir$1, "browser.js");
			const exportsCode = `module.exports = __napiModule.exports\n` + idents.map((ident) => `module.exports.${ident} = __napiModule.exports.${ident}`).join("\n");
			await writeFileAsync(bindingPath, createWasiBinding(name, this.config.packageName, (_this$config$wasm = this.config.wasm) === null || _this$config$wasm === void 0 ? void 0 : _this$config$wasm.initialMemory, (_this$config$wasm2 = this.config.wasm) === null || _this$config$wasm2 === void 0 ? void 0 : _this$config$wasm2.maximumMemory) + exportsCode + "\n", "utf8");
			await writeFileAsync(browserBindingPath, createWasiBrowserBinding(name, (_this$config$wasm3 = this.config.wasm) === null || _this$config$wasm3 === void 0 ? void 0 : _this$config$wasm3.initialMemory, (_this$config$wasm4 = this.config.wasm) === null || _this$config$wasm4 === void 0 ? void 0 : _this$config$wasm4.maximumMemory, (_this$config$wasm5 = this.config.wasm) === null || _this$config$wasm5 === void 0 || (_this$config$wasm5 = _this$config$wasm5.browser) === null || _this$config$wasm5 === void 0 ? void 0 : _this$config$wasm5.fs, (_this$config$wasm6 = this.config.wasm) === null || _this$config$wasm6 === void 0 || (_this$config$wasm6 = _this$config$wasm6.browser) === null || _this$config$wasm6 === void 0 ? void 0 : _this$config$wasm6.asyncInit, (_this$config$wasm7 = this.config.wasm) === null || _this$config$wasm7 === void 0 || (_this$config$wasm7 = _this$config$wasm7.browser) === null || _this$config$wasm7 === void 0 ? void 0 : _this$config$wasm7.buffer) + `export default __napiModule.exports\n` + idents.map((ident) => `export const ${ident} = __napiModule.exports.${ident}`).join("\n") + "\n", "utf8");
			await writeFileAsync(workerPath, WASI_WORKER_TEMPLATE, "utf8");
			await writeFileAsync(browserWorkerPath, createWasiBrowserWorkerBinding(((_this$config$wasm8 = this.config.wasm) === null || _this$config$wasm8 === void 0 || (_this$config$wasm8 = _this$config$wasm8.browser) === null || _this$config$wasm8 === void 0 ? void 0 : _this$config$wasm8.fs) ?? false), "utf8");
			await writeFileAsync(browserEntryPath, `export * from '${this.config.packageName}-wasm32-wasi'\n`);
			return [
				{
					kind: "js",
					path: bindingPath
				},
				{
					kind: "js",
					path: browserBindingPath
				},
				{
					kind: "js",
					path: workerPath
				},
				{
					kind: "js",
					path: browserWorkerPath
				},
				{
					kind: "js",
					path: browserEntryPath
				}
			];
		}
		return [];
	}
	setEnvIfNotExists(env, value$1) {
		if (!process.env[env]) this.envs[env] = value$1;
	}
};
async function writeJsBinding(options) {
	if (!options.platform || options.noJsBinding || options.idents.length === 0) return;
	const name = options.jsBinding ?? "index.js";
	const binding = (options.esm ? createEsmBinding : createCjsBinding)(options.binaryName, options.packageName, options.idents, options.version);
	try {
		const dest = join(options.outputDir, name);
		debug$7("Writing js binding to:");
		debug$7("  %i", dest);
		await writeFileAsync(dest, binding, "utf-8");
		return {
			kind: "js",
			path: dest
		};
	} catch (e) {
		throw new Error("Failed to write js binding file", { cause: e });
	}
}
async function generateTypeDef(options) {
	if (!await dirExistsAsync(options.typeDefDir)) return {
		exports: [],
		dts: ""
	};
	let header = "";
	let dts = "";
	let exports = [];
	if (!options.noDtsHeader) {
		const dtsHeader = options.dtsHeader ?? options.configDtsHeader;
		if (options.configDtsHeaderFile) try {
			header = await readFileAsync(join(options.cwd, options.configDtsHeaderFile), "utf-8");
		} catch (e) {
			debug$7.warn(`Failed to read dts header file ${options.configDtsHeaderFile}`, e);
		}
		else if (dtsHeader) header = dtsHeader;
		else header = DEFAULT_TYPE_DEF_HEADER;
	}
	const files = await readdirAsync(options.typeDefDir, { withFileTypes: true });
	if (!files.length) {
		debug$7("No type def files found. Skip generating dts file.");
		return {
			exports: [],
			dts: ""
		};
	}
	for (const file of files) {
		if (!file.isFile()) continue;
		const { dts: fileDts, exports: fileExports } = await processTypeDef(join(options.typeDefDir, file.name), options.constEnum ?? true);
		dts += fileDts;
		exports.push(...fileExports);
	}
	if (dts.indexOf("ExternalObject<") > -1) header += `
export declare class ExternalObject<T> {
  readonly '': {
    readonly '': unique symbol
    [K: symbol]: T
  }
}
`;
	if (dts.indexOf("TypedArray") > -1) header += `
export type TypedArray = Int8Array | Uint8Array | Uint8ClampedArray | Int16Array | Uint16Array | Int32Array | Uint32Array | Float32Array | Float64Array | BigInt64Array | BigUint64Array
`;
	dts = header + dts;
	return {
		exports,
		dts
	};
}

//#endregion
//#region src/def/create-npm-dirs.ts
var BaseCreateNpmDirsCommand = class extends Command {
	static paths = [["create-npm-dirs"]];
	static usage = Command.Usage({ description: "Create npm package dirs for different platforms" });
	cwd = Option.String("--cwd", process.cwd(), { description: "The working directory of where napi command will be executed in, all other paths options are relative to this path" });
	configPath = Option.String("--config-path,-c", { description: "Path to `napi` config json file" });
	packageJsonPath = Option.String("--package-json-path", "package.json", { description: "Path to `package.json`" });
	npmDir = Option.String("--npm-dir", "npm", { description: "Path to the folder where the npm packages put" });
	dryRun = Option.Boolean("--dry-run", false, { description: "Dry run without touching file system" });
	getOptions() {
		return {
			cwd: this.cwd,
			configPath: this.configPath,
			packageJsonPath: this.packageJsonPath,
			npmDir: this.npmDir,
			dryRun: this.dryRun
		};
	}
};
function applyDefaultCreateNpmDirsOptions(options) {
	return {
		cwd: process.cwd(),
		packageJsonPath: "package.json",
		npmDir: "npm",
		dryRun: false,
		...options
	};
}

//#endregion
//#region src/api/create-npm-dirs.ts
const debug$6 = debugFactory("create-npm-dirs");
async function createNpmDirs(userOptions) {
	const options = applyDefaultCreateNpmDirsOptions(userOptions);
	async function mkdirAsync$1(dir$1) {
		debug$6("Try to create dir: %i", dir$1);
		if (options.dryRun) return;
		await mkdirAsync(dir$1, { recursive: true });
	}
	async function writeFileAsync$1(file, content) {
		debug$6("Writing file %i", file);
		if (options.dryRun) {
			debug$6(content);
			return;
		}
		await writeFileAsync(file, content);
	}
	const packageJsonPath = resolve(options.cwd, options.packageJsonPath);
	const npmPath = resolve(options.cwd, options.npmDir);
	debug$6(`Read content from [${options.configPath ?? packageJsonPath}]`);
	const { targets, binaryName, packageName, packageJson } = await readNapiConfig(packageJsonPath, options.configPath ? resolve(options.cwd, options.configPath) : void 0);
	for (const target of targets) {
		const targetDir = join(npmPath, `${target.platformArchABI}`);
		await mkdirAsync$1(targetDir);
		const binaryFileName = target.arch === "wasm32" ? `${binaryName}.${target.platformArchABI}.wasm` : `${binaryName}.${target.platformArchABI}.node`;
		const scopedPackageJson = {
			name: `${packageName}-${target.platformArchABI}`,
			version: packageJson.version,
			cpu: target.arch !== "universal" ? [target.arch] : void 0,
			main: binaryFileName,
			files: [binaryFileName],
			...pick$1(packageJson, "description", "keywords", "author", "authors", "homepage", "license", "engines", "repository", "bugs")
		};
		if (packageJson.publishConfig) scopedPackageJson.publishConfig = pick$1(packageJson.publishConfig, "registry", "access");
		if (target.arch !== "wasm32") scopedPackageJson.os = [target.platform];
		else {
			var _scopedPackageJson$fi, _scopedPackageJson$en;
			const entry = `${binaryName}.wasi.cjs`;
			scopedPackageJson.main = entry;
			scopedPackageJson.browser = `${binaryName}.wasi-browser.js`;
			(_scopedPackageJson$fi = scopedPackageJson.files) === null || _scopedPackageJson$fi === void 0 || _scopedPackageJson$fi.push(entry, scopedPackageJson.browser, `wasi-worker.mjs`, `wasi-worker-browser.mjs`);
			let needRestrictNodeVersion = true;
			if ((_scopedPackageJson$en = scopedPackageJson.engines) === null || _scopedPackageJson$en === void 0 ? void 0 : _scopedPackageJson$en.node) try {
				const { major } = parse$1(scopedPackageJson.engines.node) ?? { major: 0 };
				if (major >= 14) needRestrictNodeVersion = false;
			} catch {}
			if (needRestrictNodeVersion) scopedPackageJson.engines = { node: ">=14.0.0" };
			const wasmRuntime = await fetch(`https://registry.npmjs.org/@napi-rs/wasm-runtime`).then((res) => res.json());
			scopedPackageJson.dependencies = { "@napi-rs/wasm-runtime": `^${wasmRuntime["dist-tags"].latest}` };
		}
		if (target.abi === "gnu") scopedPackageJson.libc = ["glibc"];
		else if (target.abi === "musl") scopedPackageJson.libc = ["musl"];
		await writeFileAsync$1(join(targetDir, "package.json"), JSON.stringify(scopedPackageJson, null, 2) + "\n");
		await writeFileAsync$1(join(targetDir, "README.md"), readme(packageName, target));
		debug$6.info(`${packageName} -${target.platformArchABI} created`);
	}
}
function readme(packageName, target) {
	return `# \`${packageName}-${target.platformArchABI}\`

This is the **${target.triple}** binary for \`${packageName}\`
`;
}

//#endregion
//#region src/def/new.ts
var BaseNewCommand = class extends Command {
	static paths = [["new"]];
	static usage = Command.Usage({ description: "Create a new project with pre-configured boilerplate" });
	$$path = Option.String({ required: false });
	$$name = Option.String("--name,-n", { description: "The name of the project, default to the name of the directory if not provided" });
	minNodeApiVersion = Option.String("--min-node-api,-v", "4", {
		validator: typanion.isNumber(),
		description: "The minimum Node-API version to support"
	});
	packageManager = Option.String("--package-manager", "yarn", { description: "The package manager to use. Only support yarn 4.x for now." });
	license = Option.String("--license,-l", "MIT", { description: "License for open-sourced project" });
	targets = Option.Array("--targets,-t", [], { description: "All targets the crate will be compiled for." });
	enableDefaultTargets = Option.Boolean("--enable-default-targets", true, { description: "Whether enable default targets" });
	enableAllTargets = Option.Boolean("--enable-all-targets", false, { description: "Whether enable all targets" });
	enableTypeDef = Option.Boolean("--enable-type-def", true, { description: "Whether enable the `type-def` feature for typescript definitions auto-generation" });
	enableGithubActions = Option.Boolean("--enable-github-actions", true, { description: "Whether generate preconfigured GitHub Actions workflow" });
	testFramework = Option.String("--test-framework", "ava", { description: "The JavaScript test framework to use, only support `ava` for now" });
	dryRun = Option.Boolean("--dry-run", false, { description: "Whether to run the command in dry-run mode" });
	getOptions() {
		return {
			path: this.$$path,
			name: this.$$name,
			minNodeApiVersion: this.minNodeApiVersion,
			packageManager: this.packageManager,
			license: this.license,
			targets: this.targets,
			enableDefaultTargets: this.enableDefaultTargets,
			enableAllTargets: this.enableAllTargets,
			enableTypeDef: this.enableTypeDef,
			enableGithubActions: this.enableGithubActions,
			testFramework: this.testFramework,
			dryRun: this.dryRun
		};
	}
};
function applyDefaultNewOptions(options) {
	return {
		minNodeApiVersion: 4,
		packageManager: "yarn",
		license: "MIT",
		targets: [],
		enableDefaultTargets: true,
		enableAllTargets: false,
		enableTypeDef: true,
		enableGithubActions: true,
		testFramework: "ava",
		dryRun: false,
		...options
	};
}

//#endregion
//#region ../node_modules/@std/toml/stringify.js
function joinKeys(keys) {
	return keys.map((str) => {
		return str.length === 0 || str.match(/[^A-Za-z0-9_-]/) ? JSON.stringify(str) : str;
	}).join(".");
}
var Dumper = class {
	maxPad = 0;
	srcObject;
	output = [];
	#arrayTypeCache = /* @__PURE__ */ new Map();
	constructor(srcObjc) {
		this.srcObject = srcObjc;
	}
	dump(fmtOptions = {}) {
		this.output = this.#printObject(this.srcObject);
		this.output = this.#format(fmtOptions);
		return this.output;
	}
	#printObject(obj, keys = []) {
		const out = [];
		const props = Object.keys(obj);
		const inlineProps = [];
		const multilineProps = [];
		for (const prop of props) if (this.#isSimplySerializable(obj[prop])) inlineProps.push(prop);
		else multilineProps.push(prop);
		const sortedProps = inlineProps.concat(multilineProps);
		for (const prop of sortedProps) {
			const value$1 = obj[prop];
			if (value$1 instanceof Date) out.push(this.#dateDeclaration([prop], value$1));
			else if (typeof value$1 === "string" || value$1 instanceof RegExp) out.push(this.#strDeclaration([prop], value$1.toString()));
			else if (typeof value$1 === "number") out.push(this.#numberDeclaration([prop], value$1));
			else if (typeof value$1 === "boolean") out.push(this.#boolDeclaration([prop], value$1));
			else if (value$1 instanceof Array) {
				const arrayType = this.#getTypeOfArray(value$1);
				if (arrayType === "ONLY_PRIMITIVE") out.push(this.#arrayDeclaration([prop], value$1));
				else if (arrayType === "ONLY_OBJECT_EXCLUDING_ARRAY") for (let i = 0; i < value$1.length; i++) {
					out.push("");
					out.push(this.#headerGroup([...keys, prop]));
					out.push(...this.#printObject(value$1[i], [...keys, prop]));
				}
				else {
					const str = value$1.map((x) => this.#printAsInlineValue(x)).join(",");
					out.push(`${this.#declaration([prop])}[${str}]`);
				}
			} else if (typeof value$1 === "object") {
				out.push("");
				out.push(this.#header([...keys, prop]));
				if (value$1) {
					const toParse = value$1;
					out.push(...this.#printObject(toParse, [...keys, prop]));
				}
			}
		}
		out.push("");
		return out;
	}
	#isPrimitive(value$1) {
		return value$1 instanceof Date || value$1 instanceof RegExp || [
			"string",
			"number",
			"boolean"
		].includes(typeof value$1);
	}
	#getTypeOfArray(arr) {
		if (this.#arrayTypeCache.has(arr)) return this.#arrayTypeCache.get(arr);
		const type = this.#doGetTypeOfArray(arr);
		this.#arrayTypeCache.set(arr, type);
		return type;
	}
	#doGetTypeOfArray(arr) {
		if (!arr.length) return "ONLY_PRIMITIVE";
		const onlyPrimitive = this.#isPrimitive(arr[0]);
		if (arr[0] instanceof Array) return "MIXED";
		for (let i = 1; i < arr.length; i++) if (onlyPrimitive !== this.#isPrimitive(arr[i]) || arr[i] instanceof Array) return "MIXED";
		return onlyPrimitive ? "ONLY_PRIMITIVE" : "ONLY_OBJECT_EXCLUDING_ARRAY";
	}
	#printAsInlineValue(value$1) {
		if (value$1 instanceof Date) return `"${this.#printDate(value$1)}"`;
		else if (typeof value$1 === "string" || value$1 instanceof RegExp) return JSON.stringify(value$1.toString());
		else if (typeof value$1 === "number") return value$1;
		else if (typeof value$1 === "boolean") return value$1.toString();
		else if (value$1 instanceof Array) return `[${value$1.map((x) => this.#printAsInlineValue(x)).join(",")}]`;
		else if (typeof value$1 === "object") {
			if (!value$1) throw new Error("Should never reach");
			return `{${Object.keys(value$1).map((key) => {
				return `${joinKeys([key])} = ${this.#printAsInlineValue(value$1[key])}`;
			}).join(",")}}`;
		}
		throw new Error("Should never reach");
	}
	#isSimplySerializable(value$1) {
		return typeof value$1 === "string" || typeof value$1 === "number" || typeof value$1 === "boolean" || value$1 instanceof RegExp || value$1 instanceof Date || value$1 instanceof Array && this.#getTypeOfArray(value$1) !== "ONLY_OBJECT_EXCLUDING_ARRAY";
	}
	#header(keys) {
		return `[${joinKeys(keys)}]`;
	}
	#headerGroup(keys) {
		return `[[${joinKeys(keys)}]]`;
	}
	#declaration(keys) {
		const title = joinKeys(keys);
		if (title.length > this.maxPad) this.maxPad = title.length;
		return `${title} = `;
	}
	#arrayDeclaration(keys, value$1) {
		return `${this.#declaration(keys)}${JSON.stringify(value$1)}`;
	}
	#strDeclaration(keys, value$1) {
		return `${this.#declaration(keys)}${JSON.stringify(value$1)}`;
	}
	#numberDeclaration(keys, value$1) {
		if (Number.isNaN(value$1)) return `${this.#declaration(keys)}nan`;
		switch (value$1) {
			case Infinity: return `${this.#declaration(keys)}inf`;
			case -Infinity: return `${this.#declaration(keys)}-inf`;
			default: return `${this.#declaration(keys)}${value$1}`;
		}
	}
	#boolDeclaration(keys, value$1) {
		return `${this.#declaration(keys)}${value$1}`;
	}
	#printDate(value$1) {
		function dtPad(v, lPad = 2) {
			return v.padStart(lPad, "0");
		}
		const m = dtPad((value$1.getUTCMonth() + 1).toString());
		const d = dtPad(value$1.getUTCDate().toString());
		const h = dtPad(value$1.getUTCHours().toString());
		const min = dtPad(value$1.getUTCMinutes().toString());
		const s = dtPad(value$1.getUTCSeconds().toString());
		const ms = dtPad(value$1.getUTCMilliseconds().toString(), 3);
		return `${value$1.getUTCFullYear()}-${m}-${d}T${h}:${min}:${s}.${ms}`;
	}
	#dateDeclaration(keys, value$1) {
		return `${this.#declaration(keys)}${this.#printDate(value$1)}`;
	}
	#format(options = {}) {
		const { keyAlignment = false } = options;
		const rDeclaration = /^(\".*\"|[^=]*)\s=/;
		const out = [];
		for (let i = 0; i < this.output.length; i++) {
			const l = this.output[i];
			if (l[0] === "[" && l[1] !== "[") {
				var _this$output;
				if (this.output[i + 1] === "" && ((_this$output = this.output[i + 2]) === null || _this$output === void 0 ? void 0 : _this$output.slice(0, l.length)) === l.slice(0, -1) + ".") {
					i += 1;
					continue;
				}
				out.push(l);
			} else if (keyAlignment) {
				const m = rDeclaration.exec(l);
				if (m && m[1]) out.push(l.replace(m[1], m[1].padEnd(this.maxPad)));
				else out.push(l);
			} else out.push(l);
		}
		const cleanedOutput = [];
		for (let i = 0; i < out.length; i++) {
			const l = out[i];
			if (!(l === "" && out[i + 1] === "")) cleanedOutput.push(l);
		}
		return cleanedOutput;
	}
};
/**
* Converts an object to a {@link https://toml.io | TOML} string.
*
* @example Usage
* ```ts
* import { stringify } from "@std/toml/stringify";
* import { assertEquals } from "@std/assert";
*
* const obj = {
*   title: "TOML Example",
*   owner: {
*     name: "Bob",
*     bio: "Bob is a cool guy",
*  }
* };
* const tomlString = stringify(obj);
* assertEquals(tomlString, `title = "TOML Example"\n\n[owner]\nname = "Bob"\nbio = "Bob is a cool guy"\n`);
* ```
* @param obj Source object
* @param options Options for stringifying.
* @returns TOML string
*/ function stringify(obj, options) {
	return new Dumper(obj).dump(options).join("\n");
}

//#endregion
//#region ../node_modules/@jsr/std__collections/_utils.js
/**
* Filters the given array, removing all elements that do not match the given predicate
* **in place. This means `array` will be modified!**.
*/ function filterInPlace(array, predicate) {
	let outputIndex = 0;
	for (const cur of array) {
		if (!predicate(cur)) continue;
		array[outputIndex] = cur;
		outputIndex += 1;
	}
	array.splice(outputIndex);
	return array;
}

//#endregion
//#region ../node_modules/@jsr/std__collections/deep_merge.js
function deepMerge(record, other, options) {
	return deepMergeInternal(record, other, /* @__PURE__ */ new Set(), options);
}
function deepMergeInternal(record, other, seen, options) {
	const result = {};
	const keys = new Set([...getKeys(record), ...getKeys(other)]);
	for (const key of keys) {
		if (key === "__proto__") continue;
		const a = record[key];
		if (!Object.hasOwn(other, key)) {
			result[key] = a;
			continue;
		}
		const b = other[key];
		if (isNonNullObject(a) && isNonNullObject(b) && !seen.has(a) && !seen.has(b)) {
			seen.add(a);
			seen.add(b);
			result[key] = mergeObjects(a, b, seen, options);
			continue;
		}
		result[key] = b;
	}
	return result;
}
function mergeObjects(left, right, seen, options = {
	arrays: "merge",
	sets: "merge",
	maps: "merge"
}) {
	if (isMergeable(left) && isMergeable(right)) return deepMergeInternal(left, right, seen, options);
	if (isIterable(left) && isIterable(right)) {
		if (Array.isArray(left) && Array.isArray(right)) {
			if (options.arrays === "merge") return left.concat(right);
			return right;
		}
		if (left instanceof Map && right instanceof Map) {
			if (options.maps === "merge") return new Map([...left, ...right]);
			return right;
		}
		if (left instanceof Set && right instanceof Set) {
			if (options.sets === "merge") return new Set([...left, ...right]);
			return right;
		}
	}
	return right;
}
/**
* Test whether a value is mergeable or not
* Builtins that look like objects, null and user defined classes
* are not considered mergeable (it means that reference will be copied)
*/ function isMergeable(value$1) {
	return Object.getPrototypeOf(value$1) === Object.prototype;
}
function isIterable(value$1) {
	return typeof value$1[Symbol.iterator] === "function";
}
function isNonNullObject(value$1) {
	return value$1 !== null && typeof value$1 === "object";
}
function getKeys(record) {
	const result = Object.getOwnPropertySymbols(record);
	filterInPlace(result, (key) => Object.prototype.propertyIsEnumerable.call(record, key));
	result.push(...Object.keys(record));
	return result;
}

//#endregion
//#region ../node_modules/@std/toml/_parser.js
/**
* Copy of `import { isLeap } from "@std/datetime";` because it cannot be impoted as long as it is unstable.
*/ function isLeap(yearNumber) {
	return yearNumber % 4 === 0 && yearNumber % 100 !== 0 || yearNumber % 400 === 0;
}
var Scanner = class {
	#whitespace = /[ \t]/;
	#position = 0;
	#source;
	constructor(source) {
		this.#source = source;
	}
	get position() {
		return this.#position;
	}
	get source() {
		return this.#source;
	}
	/**
	* Get current character
	* @param index - relative index from current position
	*/ char(index = 0) {
		return this.#source[this.#position + index] ?? "";
	}
	/**
	* Get sliced string
	* @param start - start position relative from current position
	* @param end - end position relative from current position
	*/ slice(start, end) {
		return this.#source.slice(this.#position + start, this.#position + end);
	}
	/**
	* Move position to next
	*/ next(count = 1) {
		this.#position += count;
	}
	skipWhitespaces() {
		while (this.#whitespace.test(this.char()) && !this.eof()) this.next();
		if (!this.isCurrentCharEOL() && /\s/.test(this.char())) {
			const escaped = "\\u" + this.char().charCodeAt(0).toString(16);
			const position = this.#position;
			throw new SyntaxError(`Cannot parse the TOML: It contains invalid whitespace at position '${position}': \`${escaped}\``);
		}
	}
	nextUntilChar(options = { skipComments: true }) {
		while (!this.eof()) {
			const char = this.char();
			if (this.#whitespace.test(char) || this.isCurrentCharEOL()) this.next();
			else if (options.skipComments && this.char() === "#") while (!this.isCurrentCharEOL() && !this.eof()) this.next();
			else break;
		}
	}
	/**
	* Position reached EOF or not
	*/ eof() {
		return this.#position >= this.#source.length;
	}
	isCurrentCharEOL() {
		return this.char() === "\n" || this.startsWith("\r\n");
	}
	startsWith(searchString) {
		return this.#source.startsWith(searchString, this.#position);
	}
	match(regExp) {
		if (!regExp.sticky) throw new Error(`RegExp ${regExp} does not have a sticky 'y' flag`);
		regExp.lastIndex = this.#position;
		return this.#source.match(regExp);
	}
};
function success(body) {
	return {
		ok: true,
		body
	};
}
function failure() {
	return { ok: false };
}
/**
* Creates a nested object from the keys and values.
*
* e.g. `unflat(["a", "b", "c"], 1)` returns `{ a: { b: { c: 1 } } }`
*/ function unflat(keys, values = { __proto__: null }) {
	return keys.reduceRight((acc, key) => ({ [key]: acc }), values);
}
function isObject(value$1) {
	return typeof value$1 === "object" && value$1 !== null;
}
function getTargetValue(target, keys) {
	const key = keys[0];
	if (!key) throw new Error("Cannot parse the TOML: key length is not a positive number");
	return target[key];
}
function deepAssignTable(target, table$1) {
	const { keys, type, value: value$1 } = table$1;
	const currentValue = getTargetValue(target, keys);
	if (currentValue === void 0) return Object.assign(target, unflat(keys, value$1));
	if (Array.isArray(currentValue)) {
		deepAssign(currentValue.at(-1), {
			type,
			keys: keys.slice(1),
			value: value$1
		});
		return target;
	}
	if (isObject(currentValue)) {
		deepAssign(currentValue, {
			type,
			keys: keys.slice(1),
			value: value$1
		});
		return target;
	}
	throw new Error("Unexpected assign");
}
function deepAssignTableArray(target, table$1) {
	const { type, keys, value: value$1 } = table$1;
	const currentValue = getTargetValue(target, keys);
	if (currentValue === void 0) return Object.assign(target, unflat(keys, [value$1]));
	if (Array.isArray(currentValue)) {
		if (table$1.keys.length === 1) currentValue.push(value$1);
		else deepAssign(currentValue.at(-1), {
			type: table$1.type,
			keys: table$1.keys.slice(1),
			value: table$1.value
		});
		return target;
	}
	if (isObject(currentValue)) {
		deepAssign(currentValue, {
			type,
			keys: keys.slice(1),
			value: value$1
		});
		return target;
	}
	throw new Error("Unexpected assign");
}
function deepAssign(target, body) {
	switch (body.type) {
		case "Block": return deepMerge(target, body.value);
		case "Table": return deepAssignTable(target, body);
		case "TableArray": return deepAssignTableArray(target, body);
	}
}
function or(parsers) {
	return (scanner) => {
		for (const parse$3 of parsers) {
			const result = parse$3(scanner);
			if (result.ok) return result;
		}
		return failure();
	};
}
/** Join the parse results of the given parser into an array.
*
* If the parser fails at the first attempt, it will return an empty array.
*/ function join$1(parser, separator) {
	const Separator = character(separator);
	return (scanner) => {
		const out = [];
		const first = parser(scanner);
		if (!first.ok) return success(out);
		out.push(first.body);
		while (!scanner.eof()) {
			if (!Separator(scanner).ok) break;
			const result = parser(scanner);
			if (!result.ok) throw new SyntaxError(`Invalid token after "${separator}"`);
			out.push(result.body);
		}
		return success(out);
	};
}
/** Join the parse results of the given parser into an array.
*
* This requires the parser to succeed at least once.
*/ function join1(parser, separator) {
	const Separator = character(separator);
	return (scanner) => {
		const first = parser(scanner);
		if (!first.ok) return failure();
		const out = [first.body];
		while (!scanner.eof()) {
			if (!Separator(scanner).ok) break;
			const result = parser(scanner);
			if (!result.ok) throw new SyntaxError(`Invalid token after "${separator}"`);
			out.push(result.body);
		}
		return success(out);
	};
}
function kv(keyParser, separator, valueParser) {
	const Separator = character(separator);
	return (scanner) => {
		const position = scanner.position;
		const key = keyParser(scanner);
		if (!key.ok) return failure();
		if (!Separator(scanner).ok) throw new SyntaxError(`key/value pair doesn't have "${separator}"`);
		const value$1 = valueParser(scanner);
		if (!value$1.ok) {
			const lineEndIndex = scanner.source.indexOf("\n", scanner.position);
			const endPosition = lineEndIndex > 0 ? lineEndIndex : scanner.source.length;
			const line = scanner.source.slice(position, endPosition);
			throw new SyntaxError(`Cannot parse value on line '${line}'`);
		}
		return success(unflat(key.body, value$1.body));
	};
}
function merge$1(parser) {
	return (scanner) => {
		const result = parser(scanner);
		if (!result.ok) return failure();
		let body = { __proto__: null };
		for (const record of result.body) if (typeof record === "object" && record !== null) body = deepMerge(body, record);
		return success(body);
	};
}
function repeat(parser) {
	return (scanner) => {
		const body = [];
		while (!scanner.eof()) {
			const result = parser(scanner);
			if (!result.ok) break;
			body.push(result.body);
			scanner.nextUntilChar();
		}
		if (body.length === 0) return failure();
		return success(body);
	};
}
function surround(left, parser, right) {
	const Left = character(left);
	const Right = character(right);
	return (scanner) => {
		if (!Left(scanner).ok) return failure();
		const result = parser(scanner);
		if (!result.ok) throw new SyntaxError(`Invalid token after "${left}"`);
		if (!Right(scanner).ok) throw new SyntaxError(`Not closed by "${right}" after started with "${left}"`);
		return success(result.body);
	};
}
function character(str) {
	return (scanner) => {
		scanner.skipWhitespaces();
		if (!scanner.startsWith(str)) return failure();
		scanner.next(str.length);
		scanner.skipWhitespaces();
		return success(void 0);
	};
}
const BARE_KEY_REGEXP = /[A-Za-z0-9_-]+/y;
function bareKey(scanner) {
	var _scanner$match;
	scanner.skipWhitespaces();
	const key = (_scanner$match = scanner.match(BARE_KEY_REGEXP)) === null || _scanner$match === void 0 ? void 0 : _scanner$match[0];
	if (!key) return failure();
	scanner.next(key.length);
	return success(key);
}
function escapeSequence(scanner) {
	if (scanner.char() !== "\\") return failure();
	scanner.next();
	switch (scanner.char()) {
		case "b":
			scanner.next();
			return success("\b");
		case "t":
			scanner.next();
			return success("	");
		case "n":
			scanner.next();
			return success("\n");
		case "f":
			scanner.next();
			return success("\f");
		case "r":
			scanner.next();
			return success("\r");
		case "u":
		case "U": {
			const codePointLen = scanner.char() === "u" ? 4 : 6;
			const codePoint = parseInt("0x" + scanner.slice(1, 1 + codePointLen), 16);
			const str = String.fromCodePoint(codePoint);
			scanner.next(codePointLen + 1);
			return success(str);
		}
		case "\"":
			scanner.next();
			return success("\"");
		case "\\":
			scanner.next();
			return success("\\");
		default: throw new SyntaxError(`Invalid escape sequence: \\${scanner.char()}`);
	}
}
function basicString(scanner) {
	scanner.skipWhitespaces();
	if (scanner.char() !== "\"") return failure();
	scanner.next();
	const acc = [];
	while (scanner.char() !== "\"" && !scanner.eof()) {
		if (scanner.char() === "\n") throw new SyntaxError("Single-line string cannot contain EOL");
		const escapedChar = escapeSequence(scanner);
		if (escapedChar.ok) acc.push(escapedChar.body);
		else {
			acc.push(scanner.char());
			scanner.next();
		}
	}
	if (scanner.eof()) throw new SyntaxError(`Single-line string is not closed:\n${acc.join("")}`);
	scanner.next();
	return success(acc.join(""));
}
function literalString(scanner) {
	scanner.skipWhitespaces();
	if (scanner.char() !== "'") return failure();
	scanner.next();
	const acc = [];
	while (scanner.char() !== "'" && !scanner.eof()) {
		if (scanner.char() === "\n") throw new SyntaxError("Single-line string cannot contain EOL");
		acc.push(scanner.char());
		scanner.next();
	}
	if (scanner.eof()) throw new SyntaxError(`Single-line string is not closed:\n${acc.join("")}`);
	scanner.next();
	return success(acc.join(""));
}
function multilineBasicString(scanner) {
	scanner.skipWhitespaces();
	if (!scanner.startsWith("\"\"\"")) return failure();
	scanner.next(3);
	if (scanner.char() === "\n") scanner.next();
	else if (scanner.startsWith("\r\n")) scanner.next(2);
	const acc = [];
	while (!scanner.startsWith("\"\"\"") && !scanner.eof()) {
		if (scanner.startsWith("\\\n")) {
			scanner.next();
			scanner.nextUntilChar({ skipComments: false });
			continue;
		} else if (scanner.startsWith("\\\r\n")) {
			scanner.next();
			scanner.nextUntilChar({ skipComments: false });
			continue;
		}
		const escapedChar = escapeSequence(scanner);
		if (escapedChar.ok) acc.push(escapedChar.body);
		else {
			acc.push(scanner.char());
			scanner.next();
		}
	}
	if (scanner.eof()) throw new SyntaxError(`Multi-line string is not closed:\n${acc.join("")}`);
	if (scanner.char(3) === "\"") {
		acc.push("\"");
		scanner.next();
	}
	scanner.next(3);
	return success(acc.join(""));
}
function multilineLiteralString(scanner) {
	scanner.skipWhitespaces();
	if (!scanner.startsWith("'''")) return failure();
	scanner.next(3);
	if (scanner.char() === "\n") scanner.next();
	else if (scanner.startsWith("\r\n")) scanner.next(2);
	const acc = [];
	while (!scanner.startsWith("'''") && !scanner.eof()) {
		acc.push(scanner.char());
		scanner.next();
	}
	if (scanner.eof()) throw new SyntaxError(`Multi-line string is not closed:\n${acc.join("")}`);
	if (scanner.char(3) === "'") {
		acc.push("'");
		scanner.next();
	}
	scanner.next(3);
	return success(acc.join(""));
}
const BOOLEAN_REGEXP = /(?:true|false)\b/y;
function boolean(scanner) {
	scanner.skipWhitespaces();
	const match = scanner.match(BOOLEAN_REGEXP);
	if (!match) return failure();
	const string = match[0];
	scanner.next(string.length);
	return success(string === "true");
}
const INFINITY_MAP = new Map([
	["inf", Infinity],
	["+inf", Infinity],
	["-inf", -Infinity]
]);
const INFINITY_REGEXP = /[+-]?inf\b/y;
function infinity(scanner) {
	scanner.skipWhitespaces();
	const match = scanner.match(INFINITY_REGEXP);
	if (!match) return failure();
	const string = match[0];
	scanner.next(string.length);
	return success(INFINITY_MAP.get(string));
}
const NAN_REGEXP = /[+-]?nan\b/y;
function nan(scanner) {
	scanner.skipWhitespaces();
	const match = scanner.match(NAN_REGEXP);
	if (!match) return failure();
	const string = match[0];
	scanner.next(string.length);
	return success(NaN);
}
const dottedKey = join1(or([
	bareKey,
	basicString,
	literalString
]), ".");
const BINARY_REGEXP = /0b[01]+(?:_[01]+)*\b/y;
function binary(scanner) {
	var _scanner$match2;
	scanner.skipWhitespaces();
	const match = (_scanner$match2 = scanner.match(BINARY_REGEXP)) === null || _scanner$match2 === void 0 ? void 0 : _scanner$match2[0];
	if (!match) return failure();
	scanner.next(match.length);
	const value$1 = match.slice(2).replaceAll("_", "");
	const number = parseInt(value$1, 2);
	return isNaN(number) ? failure() : success(number);
}
const OCTAL_REGEXP = /0o[0-7]+(?:_[0-7]+)*\b/y;
function octal(scanner) {
	var _scanner$match3;
	scanner.skipWhitespaces();
	const match = (_scanner$match3 = scanner.match(OCTAL_REGEXP)) === null || _scanner$match3 === void 0 ? void 0 : _scanner$match3[0];
	if (!match) return failure();
	scanner.next(match.length);
	const value$1 = match.slice(2).replaceAll("_", "");
	const number = parseInt(value$1, 8);
	return isNaN(number) ? failure() : success(number);
}
const HEX_REGEXP = /0x[0-9a-f]+(?:_[0-9a-f]+)*\b/iy;
function hex(scanner) {
	var _scanner$match4;
	scanner.skipWhitespaces();
	const match = (_scanner$match4 = scanner.match(HEX_REGEXP)) === null || _scanner$match4 === void 0 ? void 0 : _scanner$match4[0];
	if (!match) return failure();
	scanner.next(match.length);
	const value$1 = match.slice(2).replaceAll("_", "");
	const number = parseInt(value$1, 16);
	return isNaN(number) ? failure() : success(number);
}
const INTEGER_REGEXP = /[+-]?(?:0|[1-9][0-9]*(?:_[0-9]+)*)\b/y;
function integer(scanner) {
	var _scanner$match5;
	scanner.skipWhitespaces();
	const match = (_scanner$match5 = scanner.match(INTEGER_REGEXP)) === null || _scanner$match5 === void 0 ? void 0 : _scanner$match5[0];
	if (!match) return failure();
	scanner.next(match.length);
	const value$1 = match.replaceAll("_", "");
	return success(parseInt(value$1, 10));
}
const FLOAT_REGEXP = /[+-]?(?:0|[1-9][0-9]*(?:_[0-9]+)*)(?:\.[0-9]+(?:_[0-9]+)*)?(?:e[+-]?[0-9]+(?:_[0-9]+)*)?\b/iy;
function float(scanner) {
	var _scanner$match6;
	scanner.skipWhitespaces();
	const match = (_scanner$match6 = scanner.match(FLOAT_REGEXP)) === null || _scanner$match6 === void 0 ? void 0 : _scanner$match6[0];
	if (!match) return failure();
	scanner.next(match.length);
	const value$1 = match.replaceAll("_", "");
	const float$1 = parseFloat(value$1);
	if (isNaN(float$1)) return failure();
	return success(float$1);
}
const DATE_TIME_REGEXP = /(?<year>\d{4})-(?<month>\d{2})-(?<day>\d{2})(?:[ 0-9TZ.:+-]+)?\b/y;
function dateTime(scanner) {
	scanner.skipWhitespaces();
	const match = scanner.match(DATE_TIME_REGEXP);
	if (!match) return failure();
	const string = match[0];
	scanner.next(string.length);
	const groups = match.groups;
	if (groups.month == "02") {
		const days = parseInt(groups.day);
		if (days > 29) throw new SyntaxError(`Invalid date string "${match}"`);
		const year = parseInt(groups.year);
		if (days > 28 && !isLeap(year)) throw new SyntaxError(`Invalid date string "${match}"`);
	}
	const date = new Date(string.trim());
	if (isNaN(date.getTime())) throw new SyntaxError(`Invalid date string "${match}"`);
	return success(date);
}
const LOCAL_TIME_REGEXP = /(\d{2}):(\d{2}):(\d{2})(?:\.[0-9]+)?\b/y;
function localTime(scanner) {
	var _scanner$match7;
	scanner.skipWhitespaces();
	const match = (_scanner$match7 = scanner.match(LOCAL_TIME_REGEXP)) === null || _scanner$match7 === void 0 ? void 0 : _scanner$match7[0];
	if (!match) return failure();
	scanner.next(match.length);
	return success(match);
}
function arrayValue(scanner) {
	scanner.skipWhitespaces();
	if (scanner.char() !== "[") return failure();
	scanner.next();
	const array = [];
	while (!scanner.eof()) {
		scanner.nextUntilChar();
		const result = value(scanner);
		if (!result.ok) break;
		array.push(result.body);
		scanner.skipWhitespaces();
		if (scanner.char() !== ",") break;
		scanner.next();
	}
	scanner.nextUntilChar();
	if (scanner.char() !== "]") throw new SyntaxError("Array is not closed");
	scanner.next();
	return success(array);
}
function inlineTable(scanner) {
	scanner.nextUntilChar();
	if (scanner.char(1) === "}") {
		scanner.next(2);
		return success({ __proto__: null });
	}
	const pairs = surround("{", join$1(pair, ","), "}")(scanner);
	if (!pairs.ok) return failure();
	let table$1 = { __proto__: null };
	for (const pair$1 of pairs.body) table$1 = deepMerge(table$1, pair$1);
	return success(table$1);
}
const value = or([
	multilineBasicString,
	multilineLiteralString,
	basicString,
	literalString,
	boolean,
	infinity,
	nan,
	dateTime,
	localTime,
	binary,
	octal,
	hex,
	float,
	integer,
	arrayValue,
	inlineTable
]);
const pair = kv(dottedKey, "=", value);
function block(scanner) {
	scanner.nextUntilChar();
	const result = merge$1(repeat(pair))(scanner);
	if (result.ok) return success({
		type: "Block",
		value: result.body
	});
	return failure();
}
const tableHeader = surround("[", dottedKey, "]");
function table(scanner) {
	scanner.nextUntilChar();
	const header = tableHeader(scanner);
	if (!header.ok) return failure();
	scanner.nextUntilChar();
	const b = block(scanner);
	return success({
		type: "Table",
		keys: header.body,
		value: b.ok ? b.body.value : { __proto__: null }
	});
}
const tableArrayHeader = surround("[[", dottedKey, "]]");
function tableArray(scanner) {
	scanner.nextUntilChar();
	const header = tableArrayHeader(scanner);
	if (!header.ok) return failure();
	scanner.nextUntilChar();
	const b = block(scanner);
	return success({
		type: "TableArray",
		keys: header.body,
		value: b.ok ? b.body.value : { __proto__: null }
	});
}
function toml(scanner) {
	const blocks = repeat(or([
		block,
		tableArray,
		table
	]))(scanner);
	if (!blocks.ok) return success({ __proto__: null });
	return success(blocks.body.reduce(deepAssign, { __proto__: null }));
}
function createParseErrorMessage(scanner, message) {
	var _lines$at;
	const lines = scanner.source.slice(0, scanner.position).split("\n");
	return `Parse error on line ${lines.length}, column ${((_lines$at = lines.at(-1)) === null || _lines$at === void 0 ? void 0 : _lines$at.length) ?? 0}: ${message}`;
}
function parserFactory(parser) {
	return (tomlString) => {
		const scanner = new Scanner(tomlString);
		try {
			const result = parser(scanner);
			if (result.ok && scanner.eof()) return result.body;
			const message = `Unexpected character: "${scanner.char()}"`;
			throw new SyntaxError(createParseErrorMessage(scanner, message));
		} catch (error) {
			if (error instanceof Error) throw new SyntaxError(createParseErrorMessage(scanner, error.message));
			throw new SyntaxError(createParseErrorMessage(scanner, "Invalid error type caught"));
		}
	};
}

//#endregion
//#region ../node_modules/@std/toml/parse.js
/**
* Parses a {@link https://toml.io | TOML} string into an object.
*
* @example Usage
* ```ts
* import { parse } from "@std/toml/parse";
* import { assertEquals } from "@std/assert";
*
* const tomlString = `title = "TOML Example"
* [owner]
* name = "Alice"
* bio = "Alice is a programmer."`;
*
* const obj = parse(tomlString);
* assertEquals(obj, { title: "TOML Example", owner: { name: "Alice", bio: "Alice is a programmer." } });
* ```
* @param tomlString TOML string to be parsed.
* @returns The parsed JS object.
*/ function parse$2(tomlString) {
	return parserFactory(toml)(tomlString);
}

//#endregion
//#region ../node_modules/empathic/resolve.mjs
/**
* Resolve an absolute path from {@link root}, but only
* if {@link input} isn't already absolute.
*
* @param input The path to resolve.
* @param root The base path; default = process.cwd()
* @returns The resolved absolute path.
*/
function absolute(input$1, root) {
	return isAbsolute(input$1) ? input$1 : resolve(root || ".", input$1);
}

//#endregion
//#region ../node_modules/empathic/walk.mjs
/**
* Get all parent directories of {@link base}.
* Stops after {@link Options['last']} is processed.
*
* @returns An array of absolute paths of all parent directories.
*/
function up(base, options) {
	let { last, cwd } = options || {};
	let tmp = absolute(base, cwd);
	let root = absolute(last || "/", cwd);
	let prev, arr = [];
	while (prev !== root) {
		arr.push(tmp);
		tmp = dirname(prev = tmp);
		if (tmp === prev) break;
	}
	return arr;
}

//#endregion
//#region ../node_modules/empathic/find.mjs
/**
* Find a directory by name, walking parent directories until found.
*
* > [NOTE]
* > This function only returns a value for directory matches.
* > A file match with the same name will be ignored.
*
* @param name The directory name to find.
* @returns The absolute path to the file, if found.
*/
function dir(name, options) {
	let dir$1, tmp;
	let start = options && options.cwd || "";
	for (dir$1 of up(start, options)) try {
		tmp = join(dir$1, name);
		if (statSync(tmp).isDirectory()) return tmp;
	} catch {}
}

//#endregion
//#region src/def/rename.ts
var BaseRenameCommand = class extends Command {
	static paths = [["rename"]];
	static usage = Command.Usage({ description: "Rename the NAPI-RS project" });
	cwd = Option.String("--cwd", process.cwd(), { description: "The working directory of where napi command will be executed in, all other paths options are relative to this path" });
	configPath = Option.String("--config-path,-c", { description: "Path to `napi` config json file" });
	packageJsonPath = Option.String("--package-json-path", "package.json", { description: "Path to `package.json`" });
	npmDir = Option.String("--npm-dir", "npm", { description: "Path to the folder where the npm packages put" });
	$$name = Option.String("--name,-n", { description: "The new name of the project" });
	binaryName = Option.String("--binary-name,-b", { description: "The new binary name *.node files" });
	packageName = Option.String("--package-name", { description: "The new package name of the project" });
	manifestPath = Option.String("--manifest-path", "Cargo.toml", { description: "Path to `Cargo.toml`" });
	repository = Option.String("--repository", { description: "The new repository of the project" });
	description = Option.String("--description", { description: "The new description of the project" });
	getOptions() {
		return {
			cwd: this.cwd,
			configPath: this.configPath,
			packageJsonPath: this.packageJsonPath,
			npmDir: this.npmDir,
			name: this.$$name,
			binaryName: this.binaryName,
			packageName: this.packageName,
			manifestPath: this.manifestPath,
			repository: this.repository,
			description: this.description
		};
	}
};
function applyDefaultRenameOptions(options) {
	return {
		cwd: process.cwd(),
		packageJsonPath: "package.json",
		npmDir: "npm",
		manifestPath: "Cargo.toml",
		...options
	};
}

//#endregion
//#region src/api/rename.ts
async function renameProject(userOptions) {
	const options = applyDefaultRenameOptions(userOptions);
	const oldName = (await readConfig(options)).binaryName;
	const packageJsonPath = resolve(options.cwd, options.packageJsonPath);
	const cargoTomlPath = resolve(options.cwd, options.manifestPath);
	const packageJsonContent = await readFileAsync(packageJsonPath, "utf8");
	const packageJsonData = JSON.parse(packageJsonContent);
	merge(merge(packageJsonData, omitBy(pick(options, [
		"name",
		"description",
		"author",
		"license"
	]), isNil)), { napi: omitBy({
		binaryName: options.binaryName,
		packageName: options.packageName
	}, isNil) });
	if (options.configPath) {
		const configPath = resolve(options.cwd, options.configPath);
		const configContent = await readFileAsync(configPath, "utf8");
		const configData = JSON.parse(configContent);
		configData.binaryName = options.binaryName;
		configData.packageName = options.packageName;
		await writeFileAsync(configPath, JSON.stringify(configData, null, 2));
	}
	await writeFileAsync(packageJsonPath, JSON.stringify(packageJsonData, null, 2));
	const cargoToml = parse$2(await readFileAsync(cargoTomlPath, "utf8"));
	if (cargoToml.package && options.binaryName) {
		const sanitizedName = options.binaryName.replace("@", "").replace("/", "_").replace(/-/g, "_").toLowerCase();
		cargoToml.package.name = sanitizedName;
	}
	await writeFileAsync(cargoTomlPath, stringify(cargoToml));
	if (oldName !== options.binaryName) {
		const githubActionsPath = dir(".github", { cwd: options.cwd });
		if (githubActionsPath) {
			const githubActionsCIYmlPath = join(githubActionsPath, "workflows", "CI.yml");
			if (existsSync(githubActionsCIYmlPath)) {
				var _githubActionsData$en;
				const githubActionsData = load(await readFileAsync(githubActionsCIYmlPath, "utf8"));
				if ((_githubActionsData$en = githubActionsData.env) === null || _githubActionsData$en === void 0 ? void 0 : _githubActionsData$en.APP_NAME) {
					githubActionsData.env.APP_NAME = options.binaryName;
					await writeFileAsync(githubActionsCIYmlPath, dump(githubActionsData, {
						lineWidth: -1,
						noRefs: true,
						sortKeys: false
					}));
				}
			}
		}
		const oldWasiBrowserBindingPath = join(options.cwd, `${oldName}.wasi-browser.js`);
		if (existsSync(oldWasiBrowserBindingPath)) await rename(oldWasiBrowserBindingPath, join(options.cwd, `${options.binaryName}.wasi-browser.js`));
		const oldWasiBindingPath = join(options.cwd, `${oldName}.wasi.cjs`);
		if (existsSync(oldWasiBindingPath)) await rename(oldWasiBindingPath, join(options.cwd, `${options.binaryName}.wasi.cjs`));
		const gitAttributesPath = join(options.cwd, ".gitattributes");
		if (existsSync(gitAttributesPath)) await writeFileAsync(gitAttributesPath, (await readFileAsync(gitAttributesPath, "utf8")).split("\n").map((line) => {
			return line.replace(`${oldName}.wasi-browser.js`, `${options.binaryName}.wasi-browser.js`).replace(`${oldName}.wasi.cjs`, `${options.binaryName}.wasi.cjs`);
		}).join("\n"));
	}
}

//#endregion
//#region src/api/new.ts
const debug$5 = debugFactory("new");
const TEMPLATE_REPOS = {
	yarn: "https://github.com/napi-rs/package-template",
	pnpm: "https://github.com/napi-rs/package-template-pnpm"
};
async function checkGitCommand() {
	try {
		await new Promise((resolve$1) => {
			const cp = exec("git --version");
			cp.on("error", () => {
				resolve$1(false);
			});
			cp.on("exit", (code) => {
				if (code === 0) resolve$1(true);
				else resolve$1(false);
			});
		});
		return true;
	} catch {
		return false;
	}
}
async function ensureCacheDir(packageManager) {
	const cacheDir = path.join(homedir(), ".napi-rs", "template", packageManager);
	await mkdirAsync(cacheDir, { recursive: true });
	return cacheDir;
}
async function downloadTemplate(packageManager, cacheDir) {
	const repoUrl = TEMPLATE_REPOS[packageManager];
	const templatePath = path.join(cacheDir, "repo");
	if (existsSync(templatePath)) {
		debug$5(`Template cache found at ${templatePath}, updating...`);
		try {
			await new Promise((resolve$1, reject) => {
				const cp = exec("git fetch origin", { cwd: templatePath });
				cp.on("error", reject);
				cp.on("exit", (code) => {
					if (code === 0) resolve$1();
					else reject(/* @__PURE__ */ new Error(`Failed to fetch latest changes, git process exited with code ${code}`));
				});
			});
			execSync("git reset --hard origin/main", {
				cwd: templatePath,
				stdio: "ignore"
			});
			debug$5("Template updated successfully");
		} catch (error) {
			debug$5(`Failed to update template: ${error}`);
			throw new Error(`Failed to update template from ${repoUrl}: ${error}`);
		}
	} else {
		debug$5(`Cloning template from ${repoUrl}...`);
		try {
			execSync(`git clone ${repoUrl} repo`, {
				cwd: cacheDir,
				stdio: "inherit"
			});
			debug$5("Template cloned successfully");
		} catch (error) {
			throw new Error(`Failed to clone template from ${repoUrl}: ${error}`);
		}
	}
}
async function copyDirectory(src, dest, includeWasiBindings) {
	await mkdirAsync(dest, { recursive: true });
	const entries = await promises.readdir(src, { withFileTypes: true });
	for (const entry of entries) {
		const srcPath = path.join(src, entry.name);
		const destPath = path.join(dest, entry.name);
		if (entry.name === ".git") continue;
		if (entry.isDirectory()) await copyDirectory(srcPath, destPath, includeWasiBindings);
		else {
			if (!includeWasiBindings && (entry.name.endsWith(".wasi-browser.js") || entry.name.endsWith(".wasi.cjs") || entry.name.endsWith("wasi-worker.browser.mjs ") || entry.name.endsWith("wasi-worker.mjs") || entry.name.endsWith("browser.js"))) continue;
			await promises.copyFile(srcPath, destPath);
		}
	}
}
async function filterTargetsInPackageJson(filePath, enabledTargets) {
	var _packageJson$napi;
	const content = await promises.readFile(filePath, "utf-8");
	const packageJson = JSON.parse(content);
	if ((_packageJson$napi = packageJson.napi) === null || _packageJson$napi === void 0 ? void 0 : _packageJson$napi.targets) packageJson.napi.targets = packageJson.napi.targets.filter((target) => enabledTargets.includes(target));
	await promises.writeFile(filePath, JSON.stringify(packageJson, null, 2) + "\n");
}
async function filterTargetsInGithubActions(filePath, enabledTargets) {
	var _yaml$jobs, _yaml$jobs5;
	const yaml = load(await promises.readFile(filePath, "utf-8"));
	const macOSAndWindowsTargets = new Set([
		"x86_64-pc-windows-msvc",
		"x86_64-pc-windows-gnu",
		"aarch64-pc-windows-msvc",
		"x86_64-apple-darwin"
	]);
	const linuxTargets = new Set([
		"x86_64-unknown-linux-gnu",
		"x86_64-unknown-linux-musl",
		"aarch64-unknown-linux-gnu",
		"aarch64-unknown-linux-musl",
		"armv7-unknown-linux-gnueabihf",
		"armv7-unknown-linux-musleabihf",
		"loongarch64-unknown-linux-gnu",
		"riscv64gc-unknown-linux-gnu",
		"powerpc64le-unknown-linux-gnu",
		"s390x-unknown-linux-gnu",
		"aarch64-linux-android",
		"armv7-linux-androideabi"
	]);
	const hasLinuxTargets = enabledTargets.some((target) => linuxTargets.has(target));
	if (yaml === null || yaml === void 0 || (_yaml$jobs = yaml.jobs) === null || _yaml$jobs === void 0 || (_yaml$jobs = _yaml$jobs.build) === null || _yaml$jobs === void 0 || (_yaml$jobs = _yaml$jobs.strategy) === null || _yaml$jobs === void 0 || (_yaml$jobs = _yaml$jobs.matrix) === null || _yaml$jobs === void 0 ? void 0 : _yaml$jobs.settings) yaml.jobs.build.strategy.matrix.settings = yaml.jobs.build.strategy.matrix.settings.filter((setting) => {
		if (setting.target) return enabledTargets.includes(setting.target);
		return true;
	});
	const jobsToRemove = [];
	if (enabledTargets.every((target) => !macOSAndWindowsTargets.has(target))) jobsToRemove.push("test-macOS-windows-binding");
	else {
		var _yaml$jobs2;
		if (yaml === null || yaml === void 0 || (_yaml$jobs2 = yaml.jobs) === null || _yaml$jobs2 === void 0 || (_yaml$jobs2 = _yaml$jobs2["test-macOS-windows-binding"]) === null || _yaml$jobs2 === void 0 || (_yaml$jobs2 = _yaml$jobs2.strategy) === null || _yaml$jobs2 === void 0 || (_yaml$jobs2 = _yaml$jobs2.matrix) === null || _yaml$jobs2 === void 0 ? void 0 : _yaml$jobs2.settings) yaml.jobs["test-macOS-windows-binding"].strategy.matrix.settings = yaml.jobs["test-macOS-windows-binding"].strategy.matrix.settings.filter((setting) => {
			if (setting.target) return enabledTargets.includes(setting.target);
			return true;
		});
	}
	if (!hasLinuxTargets) {
		var _yaml$jobs3;
		if (yaml === null || yaml === void 0 || (_yaml$jobs3 = yaml.jobs) === null || _yaml$jobs3 === void 0 ? void 0 : _yaml$jobs3["test-linux-binding"]) jobsToRemove.push("test-linux-binding");
	} else {
		var _yaml$jobs4;
		if (yaml === null || yaml === void 0 || (_yaml$jobs4 = yaml.jobs) === null || _yaml$jobs4 === void 0 || (_yaml$jobs4 = _yaml$jobs4["test-linux-binding"]) === null || _yaml$jobs4 === void 0 || (_yaml$jobs4 = _yaml$jobs4.strategy) === null || _yaml$jobs4 === void 0 || (_yaml$jobs4 = _yaml$jobs4.matrix) === null || _yaml$jobs4 === void 0 ? void 0 : _yaml$jobs4.target) yaml.jobs["test-linux-binding"].strategy.matrix.target = yaml.jobs["test-linux-binding"].strategy.matrix.target.filter((target) => {
			if (target) return enabledTargets.includes(target);
			return true;
		});
	}
	if (!enabledTargets.includes("wasm32-wasip1-threads")) jobsToRemove.push("test-wasi");
	if (!enabledTargets.includes("x86_64-unknown-freebsd")) jobsToRemove.push("build-freebsd");
	for (const [jobName, jobConfig] of Object.entries(yaml.jobs || {})) if (jobName.startsWith("test-") && jobName !== "test-macOS-windows-binding" && jobName !== "test-linux-x64-gnu-binding") {
		var _job$strategy;
		const job = jobConfig;
		if ((_job$strategy = job.strategy) === null || _job$strategy === void 0 || (_job$strategy = _job$strategy.matrix) === null || _job$strategy === void 0 || (_job$strategy = _job$strategy.settings) === null || _job$strategy === void 0 || (_job$strategy = _job$strategy[0]) === null || _job$strategy === void 0 ? void 0 : _job$strategy.target) {
			const target = job.strategy.matrix.settings[0].target;
			if (!enabledTargets.includes(target)) jobsToRemove.push(jobName);
		}
	}
	for (const jobName of jobsToRemove) delete yaml.jobs[jobName];
	if (Array.isArray((_yaml$jobs5 = yaml.jobs) === null || _yaml$jobs5 === void 0 || (_yaml$jobs5 = _yaml$jobs5.publish) === null || _yaml$jobs5 === void 0 ? void 0 : _yaml$jobs5.needs)) yaml.jobs.publish.needs = yaml.jobs.publish.needs.filter((need) => !jobsToRemove.includes(need));
	const updatedYaml = dump(yaml, {
		lineWidth: -1,
		noRefs: true,
		sortKeys: false
	});
	await promises.writeFile(filePath, updatedYaml);
}
function processOptions(options) {
	var _options$targets;
	debug$5("Processing options...");
	if (!options.path) throw new Error("Please provide the path as the argument");
	options.path = path.resolve(process.cwd(), options.path);
	debug$5(`Resolved target path to: ${options.path}`);
	if (!options.name) {
		options.name = path.parse(options.path).base;
		debug$5(`No project name provided, fix it to dir name: ${options.name}`);
	}
	if (!((_options$targets = options.targets) === null || _options$targets === void 0 ? void 0 : _options$targets.length)) if (options.enableAllTargets) {
		options.targets = AVAILABLE_TARGETS.concat();
		debug$5("Enable all targets");
	} else if (options.enableDefaultTargets) {
		options.targets = DEFAULT_TARGETS.concat();
		debug$5("Enable default targets");
	} else throw new Error("At least one target must be enabled");
	if (options.targets.some((target) => target === "wasm32-wasi-preview1-threads")) {
		if (execSync(`rustup target list`, { encoding: "utf8" }).includes("wasm32-wasip1-threads")) options.targets = options.targets.map((target) => target === "wasm32-wasi-preview1-threads" ? "wasm32-wasip1-threads" : target);
	}
	return applyDefaultNewOptions(options);
}
async function newProject(userOptions) {
	debug$5("Will create napi-rs project with given options:");
	debug$5(userOptions);
	const options = processOptions(userOptions);
	debug$5("Targets to be enabled:");
	debug$5(options.targets);
	if (!await checkGitCommand()) throw new Error("Git is not installed or not available in PATH. Please install Git to continue.");
	const packageManager = options.packageManager;
	await ensurePath(options.path, options.dryRun);
	if (!options.dryRun) try {
		const cacheDir = await ensureCacheDir(packageManager);
		await downloadTemplate(packageManager, cacheDir);
		await copyDirectory(path.join(cacheDir, "repo"), options.path, options.targets.includes("wasm32-wasip1-threads"));
		await renameProject({
			cwd: options.path,
			name: options.name,
			binaryName: getBinaryName(options.name)
		});
		const packageJsonPath = path.join(options.path, "package.json");
		if (existsSync(packageJsonPath)) await filterTargetsInPackageJson(packageJsonPath, options.targets);
		const ciPath = path.join(options.path, ".github", "workflows", "CI.yml");
		if (existsSync(ciPath) && options.enableGithubActions) await filterTargetsInGithubActions(ciPath, options.targets);
		else if (!options.enableGithubActions && existsSync(path.join(options.path, ".github"))) await promises.rm(path.join(options.path, ".github"), {
			recursive: true,
			force: true
		});
		const pkgJsonContent = await promises.readFile(packageJsonPath, "utf-8");
		const pkgJson = JSON.parse(pkgJsonContent);
		if (!pkgJson.engines) pkgJson.engines = {};
		pkgJson.engines.node = napiEngineRequirement(options.minNodeApiVersion);
		if (options.license && pkgJson.license !== options.license) pkgJson.license = options.license;
		if (options.testFramework !== "ava") debug$5(`Test framework ${options.testFramework} requested but not yet implemented`);
		await promises.writeFile(packageJsonPath, JSON.stringify(pkgJson, null, 2) + "\n");
	} catch (error) {
		throw new Error(`Failed to create project: ${error}`);
	}
	debug$5(`Project created at: ${options.path}`);
}
async function ensurePath(path$1, dryRun = false) {
	const stat$1 = await statAsync(path$1, {}).catch(() => void 0);
	if (stat$1) {
		if (stat$1.isFile()) throw new Error(`Path ${path$1} for creating new napi-rs project already exists and it's not a directory.`);
		else if (stat$1.isDirectory()) {
			if ((await readdirAsync(path$1)).length) throw new Error(`Path ${path$1} for creating new napi-rs project already exists and it's not empty.`);
		}
	}
	if (!dryRun) try {
		debug$5(`Try to create target directory: ${path$1}`);
		if (!dryRun) await mkdirAsync(path$1, { recursive: true });
	} catch (e) {
		throw new Error(`Failed to create target directory: ${path$1}`, { cause: e });
	}
}
function getBinaryName(name) {
	return name.split("/").pop();
}

//#endregion
//#region src/def/pre-publish.ts
var BasePrePublishCommand = class extends Command {
	static paths = [["pre-publish"], ["prepublish"]];
	static usage = Command.Usage({ description: "Update package.json and copy addons into per platform packages" });
	cwd = Option.String("--cwd", process.cwd(), { description: "The working directory of where napi command will be executed in, all other paths options are relative to this path" });
	configPath = Option.String("--config-path,-c", { description: "Path to `napi` config json file" });
	packageJsonPath = Option.String("--package-json-path", "package.json", { description: "Path to `package.json`" });
	npmDir = Option.String("--npm-dir,-p", "npm", { description: "Path to the folder where the npm packages put" });
	tagStyle = Option.String("--tag-style,--tagstyle,-t", "lerna", { description: "git tag style, `npm` or `lerna`" });
	ghRelease = Option.Boolean("--gh-release", true, { description: "Whether create GitHub release" });
	ghReleaseName = Option.String("--gh-release-name", { description: "GitHub release name" });
	ghReleaseId = Option.String("--gh-release-id", { description: "Existing GitHub release id" });
	skipOptionalPublish = Option.Boolean("--skip-optional-publish", false, { description: "Whether skip optionalDependencies packages publish" });
	dryRun = Option.Boolean("--dry-run", false, { description: "Dry run without touching file system" });
	getOptions() {
		return {
			cwd: this.cwd,
			configPath: this.configPath,
			packageJsonPath: this.packageJsonPath,
			npmDir: this.npmDir,
			tagStyle: this.tagStyle,
			ghRelease: this.ghRelease,
			ghReleaseName: this.ghReleaseName,
			ghReleaseId: this.ghReleaseId,
			skipOptionalPublish: this.skipOptionalPublish,
			dryRun: this.dryRun
		};
	}
};
function applyDefaultPrePublishOptions(options) {
	return {
		cwd: process.cwd(),
		packageJsonPath: "package.json",
		npmDir: "npm",
		tagStyle: "lerna",
		ghRelease: true,
		skipOptionalPublish: false,
		dryRun: false,
		...options
	};
}

//#endregion
//#region src/def/version.ts
var BaseVersionCommand = class extends Command {
	static paths = [["version"]];
	static usage = Command.Usage({ description: "Update version in created npm packages" });
	cwd = Option.String("--cwd", process.cwd(), { description: "The working directory of where napi command will be executed in, all other paths options are relative to this path" });
	configPath = Option.String("--config-path,-c", { description: "Path to `napi` config json file" });
	packageJsonPath = Option.String("--package-json-path", "package.json", { description: "Path to `package.json`" });
	npmDir = Option.String("--npm-dir", "npm", { description: "Path to the folder where the npm packages put" });
	getOptions() {
		return {
			cwd: this.cwd,
			configPath: this.configPath,
			packageJsonPath: this.packageJsonPath,
			npmDir: this.npmDir
		};
	}
};
function applyDefaultVersionOptions(options) {
	return {
		cwd: process.cwd(),
		packageJsonPath: "package.json",
		npmDir: "npm",
		...options
	};
}

//#endregion
//#region src/api/version.ts
const debug$4 = debugFactory("version");
async function version(userOptions) {
	const options = applyDefaultVersionOptions(userOptions);
	const config = await readNapiConfig(resolve(options.cwd, options.packageJsonPath), options.configPath ? resolve(options.cwd, options.configPath) : void 0);
	for (const target of config.targets) {
		const pkgDir = resolve(options.cwd, options.npmDir, target.platformArchABI);
		debug$4(`Update version to %i in [%i]`, config.packageJson.version, pkgDir);
		await updatePackageJson(join(pkgDir, "package.json"), { version: config.packageJson.version });
	}
}

//#endregion
//#region src/api/pre-publish.ts
const debug$3 = debugFactory("pre-publish");
async function prePublish(userOptions) {
	debug$3("Receive pre-publish options:");
	debug$3("  %O", userOptions);
	const options = applyDefaultPrePublishOptions(userOptions);
	const packageJsonPath = resolve(options.cwd, options.packageJsonPath);
	const { packageJson, targets, packageName, binaryName, npmClient } = await readNapiConfig(packageJsonPath, options.configPath ? resolve(options.cwd, options.configPath) : void 0);
	async function createGhRelease(packageName$1, version$2) {
		if (!options.ghRelease) return {
			owner: null,
			repo: null,
			pkgInfo: {
				name: null,
				version: null,
				tag: null
			}
		};
		const { repo: repo$1, owner: owner$1, pkgInfo: pkgInfo$1, octokit: octokit$1 } = getRepoInfo(packageName$1, version$2);
		if (!repo$1 || !owner$1) return {
			owner: null,
			repo: null,
			pkgInfo: {
				name: null,
				version: null,
				tag: null
			}
		};
		if (!options.dryRun) try {
			await octokit$1.repos.createRelease({
				owner: owner$1,
				repo: repo$1,
				tag_name: pkgInfo$1.tag,
				name: options.ghReleaseName,
				prerelease: version$2.includes("alpha") || version$2.includes("beta") || version$2.includes("rc")
			});
		} catch (e) {
			debug$3(`Params: ${JSON.stringify({
				owner: owner$1,
				repo: repo$1,
				tag_name: pkgInfo$1.tag
			}, null, 2)}`);
			console.error(e);
		}
		return {
			owner: owner$1,
			repo: repo$1,
			pkgInfo: pkgInfo$1,
			octokit: octokit$1
		};
	}
	function getRepoInfo(packageName$1, version$2) {
		const headCommit = execSync("git log -1 --pretty=%B", { encoding: "utf-8" }).trim();
		const { GITHUB_REPOSITORY } = process.env;
		if (!GITHUB_REPOSITORY) return {
			owner: null,
			repo: null,
			pkgInfo: {
				name: null,
				version: null,
				tag: null
			}
		};
		debug$3(`Github repository: ${GITHUB_REPOSITORY}`);
		const [owner$1, repo$1] = GITHUB_REPOSITORY.split("/");
		const octokit$1 = new Octokit({ auth: process.env.GITHUB_TOKEN });
		let pkgInfo$1;
		if (options.tagStyle === "lerna") {
			pkgInfo$1 = headCommit.split("\n").map((line) => line.trim()).filter((line, index) => line.length && index).map((line) => line.substring(2)).map(parseTag).find((pkgInfo$2) => pkgInfo$2.name === packageName$1);
			if (!pkgInfo$1) throw new TypeError(`No release commit found with ${packageName$1}, original commit info: ${headCommit}`);
		} else pkgInfo$1 = {
			tag: `v${version$2}`,
			version: version$2,
			name: packageName$1
		};
		return {
			owner: owner$1,
			repo: repo$1,
			pkgInfo: pkgInfo$1,
			octokit: octokit$1
		};
	}
	if (!options.dryRun) {
		await version(userOptions);
		await updatePackageJson(packageJsonPath, { optionalDependencies: targets.reduce((deps, target) => {
			deps[`${packageName}-${target.platformArchABI}`] = packageJson.version;
			return deps;
		}, {}) });
	}
	const { owner, repo, pkgInfo, octokit } = options.ghReleaseId ? getRepoInfo(packageName, packageJson.version) : await createGhRelease(packageName, packageJson.version);
	for (const target of targets) {
		const pkgDir = resolve(options.cwd, options.npmDir, `${target.platformArchABI}`);
		const ext = target.platform === "wasi" || target.platform === "wasm" ? "wasm" : "node";
		const filename = `${binaryName}.${target.platformArchABI}.${ext}`;
		const dstPath = join(pkgDir, filename);
		if (!options.dryRun) {
			if (!existsSync(dstPath)) {
				debug$3.warn(`%s doesn't exist`, dstPath);
				continue;
			}
			if (!options.skipOptionalPublish) try {
				const output = execSync(`${npmClient} publish`, {
					cwd: pkgDir,
					env: process.env,
					stdio: "pipe"
				});
				process.stdout.write(output);
			} catch (e) {
				if (e instanceof Error && e.message.includes("You cannot publish over the previously published versions")) {
					console.info(e.message);
					debug$3.warn(`${pkgDir} has been published, skipping`);
				} else throw e;
			}
			if (options.ghRelease && repo && owner) {
				debug$3.info(`Creating GitHub release ${pkgInfo.tag}`);
				try {
					const releaseId = options.ghReleaseId ? Number(options.ghReleaseId) : (await octokit.repos.getReleaseByTag({
						repo,
						owner,
						tag: pkgInfo.tag
					})).data.id;
					const dstFileStats = statSync(dstPath);
					const assetInfo = await octokit.repos.uploadReleaseAsset({
						owner,
						repo,
						name: filename,
						release_id: releaseId,
						mediaType: { format: "raw" },
						headers: {
							"content-length": dstFileStats.size,
							"content-type": "application/octet-stream"
						},
						data: await readFileAsync(dstPath)
					});
					debug$3.info(`GitHub release created`);
					debug$3.info(`Download URL: %s`, assetInfo.data.browser_download_url);
				} catch (e) {
					debug$3.error(`Param: ${JSON.stringify({
						owner,
						repo,
						tag: pkgInfo.tag,
						filename: dstPath
					}, null, 2)}`);
					debug$3.error(e);
				}
			}
		}
	}
}
function parseTag(tag) {
	const segments = tag.split("@");
	const version$2 = segments.pop();
	return {
		name: segments.join("@"),
		version: version$2,
		tag
	};
}

//#endregion
//#region src/def/universalize.ts
var BaseUniversalizeCommand = class extends Command {
	static paths = [["universalize"]];
	static usage = Command.Usage({ description: "Combile built binaries into one universal binary" });
	cwd = Option.String("--cwd", process.cwd(), { description: "The working directory of where napi command will be executed in, all other paths options are relative to this path" });
	configPath = Option.String("--config-path,-c", { description: "Path to `napi` config json file" });
	packageJsonPath = Option.String("--package-json-path", "package.json", { description: "Path to `package.json`" });
	outputDir = Option.String("--output-dir,-o", "./", { description: "Path to the folder where all built `.node` files put, same as `--output-dir` of build command" });
	getOptions() {
		return {
			cwd: this.cwd,
			configPath: this.configPath,
			packageJsonPath: this.packageJsonPath,
			outputDir: this.outputDir
		};
	}
};
function applyDefaultUniversalizeOptions(options) {
	return {
		cwd: process.cwd(),
		packageJsonPath: "package.json",
		outputDir: "./",
		...options
	};
}

//#endregion
//#region src/api/universalize.ts
const debug$2 = debugFactory("universalize");
const universalizers = { darwin: (inputs, output) => {
	spawnSync("lipo", [
		"-create",
		"-output",
		output,
		...inputs
	], { stdio: "inherit" });
} };
async function universalizeBinaries(userOptions) {
	var _UniArchsByPlatform$p, _universalizers$proce;
	const options = applyDefaultUniversalizeOptions(userOptions);
	const config = await readNapiConfig(join(options.cwd, options.packageJsonPath), options.configPath ? resolve(options.cwd, options.configPath) : void 0);
	if (!config.targets.find((t) => t.platform === process.platform && t.arch === "universal")) throw new Error(`'universal' arch for platform '${process.platform}' not found in config!`);
	const srcFiles = (_UniArchsByPlatform$p = UniArchsByPlatform[process.platform]) === null || _UniArchsByPlatform$p === void 0 ? void 0 : _UniArchsByPlatform$p.map((arch) => resolve(options.cwd, options.outputDir, `${config.binaryName}.${process.platform}-${arch}.node`));
	if (!srcFiles || !universalizers[process.platform]) throw new Error(`'universal' arch for platform '${process.platform}' not supported.`);
	debug$2(`Looking up source binaries to combine: `);
	debug$2("  %O", srcFiles);
	const srcFileLookup = await Promise.all(srcFiles.map((f) => fileExists(f)));
	const notFoundFiles = srcFiles.filter((_, i) => !srcFileLookup[i]);
	if (notFoundFiles.length) throw new Error(`Some binary files were not found: ${JSON.stringify(notFoundFiles)}`);
	const output = resolve(options.cwd, options.outputDir, `${config.binaryName}.${process.platform}-universal.node`);
	(_universalizers$proce = universalizers[process.platform]) === null || _universalizers$proce === void 0 || _universalizers$proce.call(universalizers, srcFiles, output);
	debug$2(`Produced universal binary: ${output}`);
}

//#endregion
//#region src/commands/artifacts.ts
var ArtifactsCommand = class extends BaseArtifactsCommand {
	static usage = Command.Usage({
		description: "Copy artifacts from Github Actions into specified dir",
		examples: [["$0 artifacts --output-dir ./artifacts --dist ./npm", `Copy [binaryName].[platform].node under current dir(.) into packages under npm dir.
e.g: index.linux-x64-gnu.node --> ./npm/linux-x64-gnu/index.linux-x64-gnu.node`]]
	});
	static paths = [["artifacts"]];
	async execute() {
		await collectArtifacts(this.getOptions());
	}
};

//#endregion
//#region src/def/build.ts
var BaseBuildCommand = class extends Command {
	static paths = [["build"]];
	static usage = Command.Usage({ description: "Build the NAPI-RS project" });
	target = Option.String("--target,-t", { description: "Build for the target triple, bypassed to `cargo build --target`" });
	cwd = Option.String("--cwd", { description: "The working directory of where napi command will be executed in, all other paths options are relative to this path" });
	manifestPath = Option.String("--manifest-path", { description: "Path to `Cargo.toml`" });
	configPath = Option.String("--config-path,-c", { description: "Path to `napi` config json file" });
	packageJsonPath = Option.String("--package-json-path", { description: "Path to `package.json`" });
	targetDir = Option.String("--target-dir", { description: "Directory for all crate generated artifacts, see `cargo build --target-dir`" });
	outputDir = Option.String("--output-dir,-o", { description: "Path to where all the built files would be put. Default to the crate folder" });
	platform = Option.Boolean("--platform", { description: "Add platform triple to the generated nodejs binding file, eg: `[name].linux-x64-gnu.node`" });
	jsPackageName = Option.String("--js-package-name", { description: "Package name in generated js binding file. Only works with `--platform` flag" });
	constEnum = Option.Boolean("--const-enum", { description: "Whether generate const enum for typescript bindings" });
	jsBinding = Option.String("--js", { description: "Path and filename of generated JS binding file. Only works with `--platform` flag. Relative to `--output-dir`." });
	noJsBinding = Option.Boolean("--no-js", { description: "Whether to disable the generation JS binding file. Only works with `--platform` flag." });
	dts = Option.String("--dts", { description: "Path and filename of generated type def file. Relative to `--output-dir`" });
	dtsHeader = Option.String("--dts-header", { description: "Custom file header for generated type def file. Only works when `typedef` feature enabled." });
	noDtsHeader = Option.Boolean("--no-dts-header", { description: "Whether to disable the default file header for generated type def file. Only works when `typedef` feature enabled." });
	dtsCache = Option.Boolean("--dts-cache", true, { description: "Whether to enable the dts cache, default to true" });
	esm = Option.Boolean("--esm", { description: "Whether to emit an ESM JS binding file instead of CJS format. Only works with `--platform` flag." });
	strip = Option.Boolean("--strip,-s", { description: "Whether strip the library to achieve the minimum file size" });
	release = Option.Boolean("--release,-r", { description: "Build in release mode" });
	verbose = Option.Boolean("--verbose,-v", { description: "Verbosely log build command trace" });
	bin = Option.String("--bin", { description: "Build only the specified binary" });
	package = Option.String("--package,-p", { description: "Build the specified library or the one at cwd" });
	profile = Option.String("--profile", { description: "Build artifacts with the specified profile" });
	crossCompile = Option.Boolean("--cross-compile,-x", { description: "[experimental] cross-compile for the specified target with `cargo-xwin` on windows and `cargo-zigbuild` on other platform" });
	useCross = Option.Boolean("--use-cross", { description: "[experimental] use [cross](https://github.com/cross-rs/cross) instead of `cargo`" });
	useNapiCross = Option.Boolean("--use-napi-cross", { description: "[experimental] use @napi-rs/cross-toolchain to cross-compile Linux arm/arm64/x64 gnu targets." });
	watch = Option.Boolean("--watch,-w", { description: "watch the crate changes and build continuously with `cargo-watch` crates" });
	features = Option.Array("--features,-F", { description: "Space-separated list of features to activate" });
	allFeatures = Option.Boolean("--all-features", { description: "Activate all available features" });
	noDefaultFeatures = Option.Boolean("--no-default-features", { description: "Do not activate the `default` feature" });
	getOptions() {
		return {
			target: this.target,
			cwd: this.cwd,
			manifestPath: this.manifestPath,
			configPath: this.configPath,
			packageJsonPath: this.packageJsonPath,
			targetDir: this.targetDir,
			outputDir: this.outputDir,
			platform: this.platform,
			jsPackageName: this.jsPackageName,
			constEnum: this.constEnum,
			jsBinding: this.jsBinding,
			noJsBinding: this.noJsBinding,
			dts: this.dts,
			dtsHeader: this.dtsHeader,
			noDtsHeader: this.noDtsHeader,
			dtsCache: this.dtsCache,
			esm: this.esm,
			strip: this.strip,
			release: this.release,
			verbose: this.verbose,
			bin: this.bin,
			package: this.package,
			profile: this.profile,
			crossCompile: this.crossCompile,
			useCross: this.useCross,
			useNapiCross: this.useNapiCross,
			watch: this.watch,
			features: this.features,
			allFeatures: this.allFeatures,
			noDefaultFeatures: this.noDefaultFeatures
		};
	}
};

//#endregion
//#region src/commands/build.ts
const debug$1 = debugFactory("build");
var BuildCommand = class extends BaseBuildCommand {
	pipe = Option.String("--pipe", { description: "Pipe all outputs file to given command. e.g. `napi build --pipe \"npx prettier --write\"`" });
	cargoOptions = Option.Rest();
	async execute() {
		const { task } = await buildProject({
			...this.getOptions(),
			cargoOptions: this.cargoOptions
		});
		const outputs = await task;
		if (this.pipe) for (const output of outputs) {
			debug$1("Piping output file to command: %s", this.pipe);
			try {
				execSync(`${this.pipe} ${output.path}`, {
					stdio: "inherit",
					cwd: this.cwd
				});
			} catch (e) {
				debug$1.error(`Failed to pipe output file ${output.path} to command`);
				debug$1.error(e);
			}
		}
	}
};

//#endregion
//#region src/commands/create-npm-dirs.ts
var CreateNpmDirsCommand = class extends BaseCreateNpmDirsCommand {
	async execute() {
		await createNpmDirs(this.getOptions());
	}
};

//#endregion
//#region src/commands/help.ts
/**
* A command that prints the usage of all commands.
*
* Paths: `-h`, `--help`
*/
var HelpCommand = class extends Command {
	static paths = [[`-h`], [`--help`]];
	async execute() {
		await this.context.stdout.write(this.cli.usage());
	}
};

//#endregion
//#region src/commands/new.ts
const debug = debugFactory("new");
var NewCommand = class extends BaseNewCommand {
	interactive = Option.Boolean("--interactive,-i", true, { description: "Ask project basic information interactively without just using the default." });
	async execute() {
		try {
			await newProject(await this.fetchOptions());
			return 0;
		} catch (e) {
			debug("Failed to create new project");
			debug.error(e);
			return 1;
		}
	}
	async fetchOptions() {
		const cmdOptions = super.getOptions();
		if (this.interactive) {
			const targetPath = cmdOptions.path ? cmdOptions.path : await inquirerProjectPath();
			cmdOptions.path = targetPath;
			return {
				...cmdOptions,
				name: await this.fetchName(path.parse(targetPath).base),
				minNodeApiVersion: await this.fetchNapiVersion(),
				targets: await this.fetchTargets(),
				license: await this.fetchLicense(),
				enableTypeDef: await this.fetchTypeDef(),
				enableGithubActions: await this.fetchGithubActions()
			};
		}
		return cmdOptions;
	}
	async fetchName(defaultName) {
		return this.$$name ?? input({
			message: "Package name (the name field in your package.json file)",
			default: defaultName
		});
	}
	async fetchLicense() {
		return input({
			message: "License for open-sourced project",
			default: this.license
		});
	}
	async fetchNapiVersion() {
		return select({
			message: "Minimum node-api version (with node version requirement)",
			loop: false,
			pageSize: 10,
			choices: Array.from({ length: 8 }, (_, i) => ({
				name: `napi${i + 1} (${napiEngineRequirement(i + 1)})`,
				value: i + 1
			})),
			default: this.minNodeApiVersion - 1
		});
	}
	async fetchTargets() {
		if (this.enableAllTargets) return AVAILABLE_TARGETS.concat();
		return await checkbox({
			loop: false,
			message: "Choose target(s) your crate will be compiled to",
			choices: AVAILABLE_TARGETS.map((target) => ({
				name: target,
				value: target,
				checked: DEFAULT_TARGETS.includes(target)
			}))
		});
	}
	async fetchTypeDef() {
		return await confirm({
			message: "Enable type definition auto-generation",
			default: this.enableTypeDef
		});
	}
	async fetchGithubActions() {
		return await confirm({
			message: "Enable Github Actions CI",
			default: this.enableGithubActions
		});
	}
};
async function inquirerProjectPath() {
	return input({ message: "Target path to create the project, relative to cwd." }).then((path$1) => {
		if (!path$1) return inquirerProjectPath();
		return path$1;
	});
}

//#endregion
//#region src/commands/pre-publish.ts
var PrePublishCommand = class extends BasePrePublishCommand {
	async execute() {
		await prePublish(this.getOptions());
	}
};

//#endregion
//#region src/commands/rename.ts
var RenameCommand = class extends BaseRenameCommand {
	async execute() {
		const options = this.getOptions();
		if (!options.name) options.name = await input({
			message: `Enter the new package name in the package.json`,
			required: true
		});
		if (!options.binaryName) options.binaryName = await input({
			message: `Enter the new binary name`,
			required: true
		});
		await renameProject(options);
	}
};

//#endregion
//#region src/commands/universalize.ts
var UniversalizeCommand = class extends BaseUniversalizeCommand {
	async execute() {
		await universalizeBinaries(this.getOptions());
	}
};

//#endregion
//#region src/commands/version.ts
var VersionCommand = class extends BaseVersionCommand {
	async execute() {
		await version(this.getOptions());
	}
};

//#endregion
//#region src/index.ts
const cli = new Cli({
	binaryName: "napi",
	binaryVersion: CLI_VERSION
});
cli.register(NewCommand);
cli.register(BuildCommand);
cli.register(CreateNpmDirsCommand);
cli.register(ArtifactsCommand);
cli.register(UniversalizeCommand);
cli.register(RenameCommand);
cli.register(PrePublishCommand);
cli.register(VersionCommand);
cli.register(HelpCommand);
/**
*
* @usage
*
* ```ts
* const cli = new NapiCli()
*
* cli.build({
*   cwd: '/path/to/your/project',
* })
* ```
*/
var NapiCli = class {
	artifacts = collectArtifacts;
	new = newProject;
	build = buildProject;
	createNpmDirs = createNpmDirs;
	prePublish = prePublish;
	rename = renameProject;
	universalize = universalizeBinaries;
	version = version;
};
function createBuildCommand(args) {
	return cli.process(["build", ...args]);
}
function createArtifactsCommand(args) {
	return cli.process(["artifacts", ...args]);
}
function createCreateNpmDirsCommand(args) {
	return cli.process(["create-npm-dirs", ...args]);
}
function createPrePublishCommand(args) {
	return cli.process(["pre-publish", ...args]);
}
function createRenameCommand(args) {
	return cli.process(["rename", ...args]);
}
function createUniversalizeCommand(args) {
	return cli.process(["universalize", ...args]);
}
function createVersionCommand(args) {
	return cli.process(["version", ...args]);
}
function createNewCommand(args) {
	return cli.process(["new", ...args]);
}

//#endregion
export { NapiCli, cli, createArtifactsCommand, createBuildCommand, createCreateNpmDirsCommand, createNewCommand, createPrePublishCommand, createRenameCommand, createUniversalizeCommand, createVersionCommand, generateTypeDef, parseTriple, readNapiConfig, writeJsBinding };
//# sourceMappingURL=data:application/json;charset=utf-8;base64,eyJ2ZXJzaW9uIjozLCJmaWxlIjoiaW5kZXguanMiLCJuYW1lcyI6WyJkZWJ1ZyIsInBhdGgiLCJwaWNrIiwicGtnSnNvbi52ZXJzaW9uIiwiVEFSR0VUX0xJTktFUjogUmVjb3JkPHN0cmluZywgc3RyaW5nPiIsIkNwdVRvTm9kZUFyY2g6IFJlY29yZDxzdHJpbmcsIE5vZGVKU0FyY2g+IiwiU3lzVG9Ob2RlUGxhdGZvcm06IFJlY29yZDxzdHJpbmcsIFBsYXRmb3JtPiIsIlVuaUFyY2hzQnlQbGF0Zm9ybTogUGFydGlhbDxSZWNvcmQ8UGxhdGZvcm0sIE5vZGVKU0FyY2hbXT4+IiwiY3B1OiBzdHJpbmciLCJzeXM6IHN0cmluZyIsImFiaTogc3RyaW5nIHwgbnVsbCIsInJlcXVpcmVtZW50czogc3RyaW5nW10iLCJwYXRoIiwic2VwYXJhdGVkQ29uZmlnOiBVc2VyTmFwaUNvbmZpZyB8IHVuZGVmaW5lZCIsIm5hcGlDb25maWc6IE5hcGlDb25maWciLCJ0YXJnZXRzOiBzdHJpbmdbXSIsImV4cG9ydHM6IHN0cmluZ1tdIiwiZGVidWciLCJkaXIiLCJmcyIsImZzIiwiZGVidWciLCJvcHRpb25zOiBQYXJzZWRCdWlsZE9wdGlvbnMiLCJtZXRhZGF0YTogQ2FyZ29Xb3Jrc3BhY2VNZXRhZGF0YSIsImNyYXRlOiBDcmF0ZSIsImNvbmZpZzogTmFwaUNvbmZpZyIsImFsaWFzOiBSZWNvcmQ8c3RyaW5nLCBzdHJpbmc+IiwidmVyc2lvbiIsImRlc3QiLCJkaXIiLCJ2YWx1ZSIsImV4cG9ydHM6IHN0cmluZ1tdIiwiZGVidWciLCJta2RpckFzeW5jIiwiZGlyIiwicmF3TWtkaXJBc3luYyIsIndyaXRlRmlsZUFzeW5jIiwicmF3V3JpdGVGaWxlQXN5bmMiLCJzY29wZWRQYWNrYWdlSnNvbjogQ29tbW9uUGFja2FnZUpzb25GaWVsZHMiLCJwaWNrIiwicGFyc2UiLCIjcHJpbnRPYmplY3QiLCIjZm9ybWF0IiwiI2lzU2ltcGx5U2VyaWFsaXphYmxlIiwidmFsdWUiLCIjZGF0ZURlY2xhcmF0aW9uIiwiI3N0ckRlY2xhcmF0aW9uIiwiI251bWJlckRlY2xhcmF0aW9uIiwiI2Jvb2xEZWNsYXJhdGlvbiIsIiNnZXRUeXBlT2ZBcnJheSIsIiNhcnJheURlY2xhcmF0aW9uIiwiI2hlYWRlckdyb3VwIiwiI3ByaW50QXNJbmxpbmVWYWx1ZSIsIiNkZWNsYXJhdGlvbiIsIiNoZWFkZXIiLCIjYXJyYXlUeXBlQ2FjaGUiLCIjZG9HZXRUeXBlT2ZBcnJheSIsIiNpc1ByaW1pdGl2ZSIsIiNwcmludERhdGUiLCJ2YWx1ZSIsIiNzb3VyY2UiLCIjcG9zaXRpb24iLCIjd2hpdGVzcGFjZSIsInZhbHVlIiwidGFibGUiLCJwYXJzZSIsImpvaW4iLCJtZXJnZSIsImZsb2F0IiwicGFpciIsInBhcnNlIiwiaW5wdXQiLCJkaXIiLCJ3YWxrLnVwIiwicGFyc2VUb21sIiwic3RyaW5naWZ5VG9tbCIsImZpbmQuZGlyIiwieWFtbFBhcnNlIiwieWFtbFN0cmluZ2lmeSIsImRlYnVnIiwiZnMiLCJ5YW1sTG9hZCIsImpvYnNUb1JlbW92ZTogc3RyaW5nW10iLCJ5YW1sRHVtcCIsInN0YXQiLCJwYXRoIiwiZGVidWciLCJkZWJ1ZyIsInBhY2thZ2VOYW1lIiwidmVyc2lvbiIsInJlcG8iLCJvd25lciIsIm9jdG9raXQiLCJwa2dJbmZvIiwicGtnSW5mbzogUGFja2FnZUluZm8gfCB1bmRlZmluZWQiLCJkZWJ1ZyIsInVuaXZlcnNhbGl6ZXJzOiBQYXJ0aWFsPFxuICBSZWNvcmQ8Tm9kZUpTLlBsYXRmb3JtLCAoaW5wdXRzOiBzdHJpbmdbXSwgb3V0cHV0OiBzdHJpbmcpID0+IHZvaWQ+XG4+IiwiZGVidWciLCJ0YXJnZXRQYXRoOiBzdHJpbmciLCJwYXRoIl0sInNvdXJjZXMiOlsiLi4vc3JjL2RlZi9hcnRpZmFjdHMudHMiLCIuLi9zcmMvdXRpbHMvbG9nLnRzIiwiLi4vcGFja2FnZS5qc29uIiwiLi4vc3JjL3V0aWxzL21pc2MudHMiLCIuLi9zcmMvdXRpbHMvdGFyZ2V0LnRzIiwiLi4vc3JjL3V0aWxzL3ZlcnNpb24udHMiLCIuLi9zcmMvdXRpbHMvbWV0YWRhdGEudHMiLCIuLi9zcmMvdXRpbHMvY29uZmlnLnRzIiwiLi4vc3JjL3V0aWxzL2NhcmdvLnRzIiwiLi4vc3JjL3V0aWxzL3R5cGVnZW4udHMiLCIuLi9zcmMvdXRpbHMvcmVhZC1jb25maWcudHMiLCIuLi9zcmMvYXBpL2FydGlmYWN0cy50cyIsIi4uL3NyYy9hcGkvdGVtcGxhdGVzL2pzLWJpbmRpbmcudHMiLCIuLi9zcmMvYXBpL3RlbXBsYXRlcy9sb2FkLXdhc2ktdGVtcGxhdGUudHMiLCIuLi9zcmMvYXBpL3RlbXBsYXRlcy93YXNpLXdvcmtlci10ZW1wbGF0ZS50cyIsIi4uL3NyYy9hcGkvYnVpbGQudHMiLCIuLi9zcmMvZGVmL2NyZWF0ZS1ucG0tZGlycy50cyIsIi4uL3NyYy9hcGkvY3JlYXRlLW5wbS1kaXJzLnRzIiwiLi4vc3JjL2RlZi9uZXcudHMiLCIuLi8uLi9ub2RlX21vZHVsZXMvQHN0ZC90b21sL3N0cmluZ2lmeS5qcyIsIi4uLy4uL25vZGVfbW9kdWxlcy9AanNyL3N0ZF9fY29sbGVjdGlvbnMvX3V0aWxzLmpzIiwiLi4vLi4vbm9kZV9tb2R1bGVzL0Bqc3Ivc3RkX19jb2xsZWN0aW9ucy9kZWVwX21lcmdlLmpzIiwiLi4vLi4vbm9kZV9tb2R1bGVzL0BzdGQvdG9tbC9fcGFyc2VyLmpzIiwiLi4vLi4vbm9kZV9tb2R1bGVzL0BzdGQvdG9tbC9wYXJzZS5qcyIsIi4uLy4uL25vZGVfbW9kdWxlcy9lbXBhdGhpYy9yZXNvbHZlLm1qcyIsIi4uLy4uL25vZGVfbW9kdWxlcy9lbXBhdGhpYy93YWxrLm1qcyIsIi4uLy4uL25vZGVfbW9kdWxlcy9lbXBhdGhpYy9maW5kLm1qcyIsIi4uL3NyYy9kZWYvcmVuYW1lLnRzIiwiLi4vc3JjL2FwaS9yZW5hbWUudHMiLCIuLi9zcmMvYXBpL25ldy50cyIsIi4uL3NyYy9kZWYvcHJlLXB1Ymxpc2gudHMiLCIuLi9zcmMvZGVmL3ZlcnNpb24udHMiLCIuLi9zcmMvYXBpL3ZlcnNpb24udHMiLCIuLi9zcmMvYXBpL3ByZS1wdWJsaXNoLnRzIiwiLi4vc3JjL2RlZi91bml2ZXJzYWxpemUudHMiLCIuLi9zcmMvYXBpL3VuaXZlcnNhbGl6ZS50cyIsIi4uL3NyYy9jb21tYW5kcy9hcnRpZmFjdHMudHMiLCIuLi9zcmMvZGVmL2J1aWxkLnRzIiwiLi4vc3JjL2NvbW1hbmRzL2J1aWxkLnRzIiwiLi4vc3JjL2NvbW1hbmRzL2NyZWF0ZS1ucG0tZGlycy50cyIsIi4uL3NyYy9jb21tYW5kcy9oZWxwLnRzIiwiLi4vc3JjL2NvbW1hbmRzL25ldy50cyIsIi4uL3NyYy9jb21tYW5kcy9wcmUtcHVibGlzaC50cyIsIi4uL3NyYy9jb21tYW5kcy9yZW5hbWUudHMiLCIuLi9zcmMvY29tbWFuZHMvdW5pdmVyc2FsaXplLnRzIiwiLi4vc3JjL2NvbW1hbmRzL3ZlcnNpb24udHMiLCIuLi9zcmMvaW5kZXgudHMiXSwic291cmNlc0NvbnRlbnQiOlsiLy8gVGhpcyBmaWxlIGlzIGdlbmVyYXRlZCBieSBjb2RlZ2VuL2luZGV4LnRzXG4vLyBEbyBub3QgZWRpdCB0aGlzIGZpbGUgbWFudWFsbHlcbmltcG9ydCB7IENvbW1hbmQsIE9wdGlvbiB9IGZyb20gJ2NsaXBhbmlvbidcblxuZXhwb3J0IGFic3RyYWN0IGNsYXNzIEJhc2VBcnRpZmFjdHNDb21tYW5kIGV4dGVuZHMgQ29tbWFuZCB7XG4gIHN0YXRpYyBwYXRocyA9IFtbJ2FydGlmYWN0cyddXVxuXG4gIHN0YXRpYyB1c2FnZSA9IENvbW1hbmQuVXNhZ2Uoe1xuICAgIGRlc2NyaXB0aW9uOlxuICAgICAgJ0NvcHkgYXJ0aWZhY3RzIGZyb20gR2l0aHViIEFjdGlvbnMgaW50byBucG0gcGFja2FnZXMgYW5kIHJlYWR5IHRvIHB1Ymxpc2gnLFxuICB9KVxuXG4gIGN3ZCA9IE9wdGlvbi5TdHJpbmcoJy0tY3dkJywgcHJvY2Vzcy5jd2QoKSwge1xuICAgIGRlc2NyaXB0aW9uOlxuICAgICAgJ1RoZSB3b3JraW5nIGRpcmVjdG9yeSBvZiB3aGVyZSBuYXBpIGNvbW1hbmQgd2lsbCBiZSBleGVjdXRlZCBpbiwgYWxsIG90aGVyIHBhdGhzIG9wdGlvbnMgYXJlIHJlbGF0aXZlIHRvIHRoaXMgcGF0aCcsXG4gIH0pXG5cbiAgY29uZmlnUGF0aD86IHN0cmluZyA9IE9wdGlvbi5TdHJpbmcoJy0tY29uZmlnLXBhdGgsLWMnLCB7XG4gICAgZGVzY3JpcHRpb246ICdQYXRoIHRvIGBuYXBpYCBjb25maWcganNvbiBmaWxlJyxcbiAgfSlcblxuICBwYWNrYWdlSnNvblBhdGggPSBPcHRpb24uU3RyaW5nKCctLXBhY2thZ2UtanNvbi1wYXRoJywgJ3BhY2thZ2UuanNvbicsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1BhdGggdG8gYHBhY2thZ2UuanNvbmAnLFxuICB9KVxuXG4gIG91dHB1dERpciA9IE9wdGlvbi5TdHJpbmcoJy0tb3V0cHV0LWRpciwtbywtZCcsICcuL2FydGlmYWN0cycsIHtcbiAgICBkZXNjcmlwdGlvbjpcbiAgICAgICdQYXRoIHRvIHRoZSBmb2xkZXIgd2hlcmUgYWxsIGJ1aWx0IGAubm9kZWAgZmlsZXMgcHV0LCBzYW1lIGFzIGAtLW91dHB1dC1kaXJgIG9mIGJ1aWxkIGNvbW1hbmQnLFxuICB9KVxuXG4gIG5wbURpciA9IE9wdGlvbi5TdHJpbmcoJy0tbnBtLWRpcicsICducG0nLCB7XG4gICAgZGVzY3JpcHRpb246ICdQYXRoIHRvIHRoZSBmb2xkZXIgd2hlcmUgdGhlIG5wbSBwYWNrYWdlcyBwdXQnLFxuICB9KVxuXG4gIGJ1aWxkT3V0cHV0RGlyPzogc3RyaW5nID0gT3B0aW9uLlN0cmluZygnLS1idWlsZC1vdXRwdXQtZGlyJywge1xuICAgIGRlc2NyaXB0aW9uOlxuICAgICAgJ1BhdGggdG8gdGhlIGJ1aWxkIG91dHB1dCBkaXIsIG9ubHkgbmVlZGVkIHdoZW4gdGFyZ2V0cyBjb250YWlucyBgd2FzbTMyLXdhc2ktKmAnLFxuICB9KVxuXG4gIGdldE9wdGlvbnMoKSB7XG4gICAgcmV0dXJuIHtcbiAgICAgIGN3ZDogdGhpcy5jd2QsXG4gICAgICBjb25maWdQYXRoOiB0aGlzLmNvbmZpZ1BhdGgsXG4gICAgICBwYWNrYWdlSnNvblBhdGg6IHRoaXMucGFja2FnZUpzb25QYXRoLFxuICAgICAgb3V0cHV0RGlyOiB0aGlzLm91dHB1dERpcixcbiAgICAgIG5wbURpcjogdGhpcy5ucG1EaXIsXG4gICAgICBidWlsZE91dHB1dERpcjogdGhpcy5idWlsZE91dHB1dERpcixcbiAgICB9XG4gIH1cbn1cblxuLyoqXG4gKiBDb3B5IGFydGlmYWN0cyBmcm9tIEdpdGh1YiBBY3Rpb25zIGludG8gbnBtIHBhY2thZ2VzIGFuZCByZWFkeSB0byBwdWJsaXNoXG4gKi9cbmV4cG9ydCBpbnRlcmZhY2UgQXJ0aWZhY3RzT3B0aW9ucyB7XG4gIC8qKlxuICAgKiBUaGUgd29ya2luZyBkaXJlY3Rvcnkgb2Ygd2hlcmUgbmFwaSBjb21tYW5kIHdpbGwgYmUgZXhlY3V0ZWQgaW4sIGFsbCBvdGhlciBwYXRocyBvcHRpb25zIGFyZSByZWxhdGl2ZSB0byB0aGlzIHBhdGhcbiAgICpcbiAgICogQGRlZmF1bHQgcHJvY2Vzcy5jd2QoKVxuICAgKi9cbiAgY3dkPzogc3RyaW5nXG4gIC8qKlxuICAgKiBQYXRoIHRvIGBuYXBpYCBjb25maWcganNvbiBmaWxlXG4gICAqL1xuICBjb25maWdQYXRoPzogc3RyaW5nXG4gIC8qKlxuICAgKiBQYXRoIHRvIGBwYWNrYWdlLmpzb25gXG4gICAqXG4gICAqIEBkZWZhdWx0ICdwYWNrYWdlLmpzb24nXG4gICAqL1xuICBwYWNrYWdlSnNvblBhdGg/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gdGhlIGZvbGRlciB3aGVyZSBhbGwgYnVpbHQgYC5ub2RlYCBmaWxlcyBwdXQsIHNhbWUgYXMgYC0tb3V0cHV0LWRpcmAgb2YgYnVpbGQgY29tbWFuZFxuICAgKlxuICAgKiBAZGVmYXVsdCAnLi9hcnRpZmFjdHMnXG4gICAqL1xuICBvdXRwdXREaXI/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gdGhlIGZvbGRlciB3aGVyZSB0aGUgbnBtIHBhY2thZ2VzIHB1dFxuICAgKlxuICAgKiBAZGVmYXVsdCAnbnBtJ1xuICAgKi9cbiAgbnBtRGlyPzogc3RyaW5nXG4gIC8qKlxuICAgKiBQYXRoIHRvIHRoZSBidWlsZCBvdXRwdXQgZGlyLCBvbmx5IG5lZWRlZCB3aGVuIHRhcmdldHMgY29udGFpbnMgYHdhc20zMi13YXNpLSpgXG4gICAqL1xuICBidWlsZE91dHB1dERpcj86IHN0cmluZ1xufVxuXG5leHBvcnQgZnVuY3Rpb24gYXBwbHlEZWZhdWx0QXJ0aWZhY3RzT3B0aW9ucyhvcHRpb25zOiBBcnRpZmFjdHNPcHRpb25zKSB7XG4gIHJldHVybiB7XG4gICAgY3dkOiBwcm9jZXNzLmN3ZCgpLFxuICAgIHBhY2thZ2VKc29uUGF0aDogJ3BhY2thZ2UuanNvbicsXG4gICAgb3V0cHV0RGlyOiAnLi9hcnRpZmFjdHMnLFxuICAgIG5wbURpcjogJ25wbScsXG4gICAgLi4ub3B0aW9ucyxcbiAgfVxufVxuIiwiaW1wb3J0ICogYXMgY29sb3JzIGZyb20gJ2NvbG9yZXR0ZSdcbmltcG9ydCB7IGNyZWF0ZURlYnVnIH0gZnJvbSAnb2J1ZydcblxuZGVjbGFyZSBtb2R1bGUgJ29idWcnIHtcbiAgaW50ZXJmYWNlIERlYnVnZ2VyIHtcbiAgICBpbmZvOiB0eXBlb2YgY29uc29sZS5lcnJvclxuICAgIHdhcm46IHR5cGVvZiBjb25zb2xlLmVycm9yXG4gICAgZXJyb3I6IHR5cGVvZiBjb25zb2xlLmVycm9yXG4gIH1cbn1cblxuZXhwb3J0IGNvbnN0IGRlYnVnRmFjdG9yeSA9IChuYW1lc3BhY2U6IHN0cmluZykgPT4ge1xuICBjb25zdCBkZWJ1ZyA9IGNyZWF0ZURlYnVnKGBuYXBpOiR7bmFtZXNwYWNlfWAsIHtcbiAgICBmb3JtYXR0ZXJzOiB7XG4gICAgICAvLyBkZWJ1ZygnJWknLCAnVGhpcyBpcyBhbiBpbmZvJylcbiAgICAgIGkodikge1xuICAgICAgICByZXR1cm4gY29sb3JzLmdyZWVuKHYpXG4gICAgICB9LFxuICAgIH0sXG4gIH0pXG5cbiAgZGVidWcuaW5mbyA9ICguLi5hcmdzOiBhbnlbXSkgPT5cbiAgICBjb25zb2xlLmVycm9yKGNvbG9ycy5ibGFjayhjb2xvcnMuYmdHcmVlbignIElORk8gJykpLCAuLi5hcmdzKVxuICBkZWJ1Zy53YXJuID0gKC4uLmFyZ3M6IGFueVtdKSA9PlxuICAgIGNvbnNvbGUuZXJyb3IoY29sb3JzLmJsYWNrKGNvbG9ycy5iZ1llbGxvdygnIFdBUk5JTkcgJykpLCAuLi5hcmdzKVxuICBkZWJ1Zy5lcnJvciA9ICguLi5hcmdzOiBhbnlbXSkgPT5cbiAgICBjb25zb2xlLmVycm9yKFxuICAgICAgY29sb3JzLndoaXRlKGNvbG9ycy5iZ1JlZCgnIEVSUk9SICcpKSxcbiAgICAgIC4uLmFyZ3MubWFwKChhcmcpID0+XG4gICAgICAgIGFyZyBpbnN0YW5jZW9mIEVycm9yID8gKGFyZy5zdGFjayA/PyBhcmcubWVzc2FnZSkgOiBhcmcsXG4gICAgICApLFxuICAgIClcblxuICByZXR1cm4gZGVidWdcbn1cbmV4cG9ydCBjb25zdCBkZWJ1ZyA9IGRlYnVnRmFjdG9yeSgndXRpbHMnKVxuIiwie1xuICBcIm5hbWVcIjogXCJAbmFwaS1ycy9jbGlcIixcbiAgXCJ2ZXJzaW9uXCI6IFwiMy41LjFcIixcbiAgXCJkZXNjcmlwdGlvblwiOiBcIkNsaSB0b29scyBmb3IgbmFwaS1yc1wiLFxuICBcImF1dGhvclwiOiBcIkxvbmdZaW5hbiA8bHlud2VrbG1AZ21haWwuY29tPlwiLFxuICBcImhvbWVwYWdlXCI6IFwiaHR0cHM6Ly9uYXBpLnJzL1wiLFxuICBcImxpY2Vuc2VcIjogXCJNSVRcIixcbiAgXCJ0eXBlXCI6IFwibW9kdWxlXCIsXG4gIFwiZW5naW5lc1wiOiB7XG4gICAgXCJub2RlXCI6IFwiPj0gMTZcIlxuICB9LFxuICBcImJpblwiOiB7XG4gICAgXCJuYXBpXCI6IFwiLi9kaXN0L2NsaS5qc1wiLFxuICAgIFwibmFwaS1yYXdcIjogXCIuL2NsaS5tanNcIlxuICB9LFxuICBcIm1haW5cIjogXCIuL2Rpc3QvaW5kZXguY2pzXCIsXG4gIFwibW9kdWxlXCI6IFwiLi9kaXN0L2luZGV4LmpzXCIsXG4gIFwidHlwZXNcIjogXCIuL2Rpc3QvaW5kZXguZC50c1wiLFxuICBcImV4cG9ydHNcIjoge1xuICAgIFwiLlwiOiB7XG4gICAgICBcImltcG9ydFwiOiBcIi4vZGlzdC9pbmRleC5qc1wiLFxuICAgICAgXCJyZXF1aXJlXCI6IFwiLi9kaXN0L2luZGV4LmNqc1wiXG4gICAgfSxcbiAgICBcIi4vcGFja2FnZS5qc29uXCI6IFwiLi9wYWNrYWdlLmpzb25cIlxuICB9LFxuICBcImZpbGVzXCI6IFtcbiAgICBcImRpc3RcIixcbiAgICBcInNyY1wiLFxuICAgIFwiIV9fdGVzdHNfX1wiXG4gIF0sXG4gIFwia2V5d29yZHNcIjogW1xuICAgIFwiY2xpXCIsXG4gICAgXCJydXN0XCIsXG4gICAgXCJuYXBpXCIsXG4gICAgXCJuLWFwaVwiLFxuICAgIFwibm9kZS1hcGlcIixcbiAgICBcIm5vZGUtYWRkb25cIixcbiAgICBcIm5lb25cIlxuICBdLFxuICBcIm1haW50YWluZXJzXCI6IFtcbiAgICB7XG4gICAgICBcIm5hbWVcIjogXCJMb25nWWluYW5cIixcbiAgICAgIFwiZW1haWxcIjogXCJseW53ZWtsbUBnbWFpbC5jb21cIixcbiAgICAgIFwiaG9tZXBhZ2VcIjogXCJodHRwczovL2dpdGh1Yi5jb20vQnJvb29vb29rbHluXCJcbiAgICB9LFxuICAgIHtcbiAgICAgIFwibmFtZVwiOiBcImZvcmVoYWxvXCIsXG4gICAgICBcImhvbWVwYWdlXCI6IFwiaHR0cHM6Ly9naXRodWIuY29tL2ZvcmVoYWxvXCJcbiAgICB9XG4gIF0sXG4gIFwicmVwb3NpdG9yeVwiOiB7XG4gICAgXCJ0eXBlXCI6IFwiZ2l0XCIsXG4gICAgXCJ1cmxcIjogXCJnaXQraHR0cHM6Ly9naXRodWIuY29tL25hcGktcnMvbmFwaS1ycy5naXRcIlxuICB9LFxuICBcInB1Ymxpc2hDb25maWdcIjoge1xuICAgIFwicmVnaXN0cnlcIjogXCJodHRwczovL3JlZ2lzdHJ5Lm5wbWpzLm9yZy9cIixcbiAgICBcImFjY2Vzc1wiOiBcInB1YmxpY1wiXG4gIH0sXG4gIFwiYnVnc1wiOiB7XG4gICAgXCJ1cmxcIjogXCJodHRwczovL2dpdGh1Yi5jb20vbmFwaS1ycy9uYXBpLXJzL2lzc3Vlc1wiXG4gIH0sXG4gIFwiZGVwZW5kZW5jaWVzXCI6IHtcbiAgICBcIkBpbnF1aXJlci9wcm9tcHRzXCI6IFwiXjguMC4wXCIsXG4gICAgXCJAbmFwaS1ycy9jcm9zcy10b29sY2hhaW5cIjogXCJeMS4wLjNcIixcbiAgICBcIkBuYXBpLXJzL3dhc20tdG9vbHNcIjogXCJeMS4wLjFcIixcbiAgICBcIkBvY3Rva2l0L3Jlc3RcIjogXCJeMjIuMC4xXCIsXG4gICAgXCJjbGlwYW5pb25cIjogXCJeNC4wLjAtcmMuNFwiLFxuICAgIFwiY29sb3JldHRlXCI6IFwiXjIuMC4yMFwiLFxuICAgIFwiZW1uYXBpXCI6IFwiXjEuNy4xXCIsXG4gICAgXCJlcy10b29sa2l0XCI6IFwiXjEuNDEuMFwiLFxuICAgIFwianMteWFtbFwiOiBcIl40LjEuMFwiLFxuICAgIFwib2J1Z1wiOiBcIl4yLjAuMFwiLFxuICAgIFwic2VtdmVyXCI6IFwiXjcuNy4zXCIsXG4gICAgXCJ0eXBhbmlvblwiOiBcIl4zLjE0LjBcIlxuICB9LFxuICBcImRldkRlcGVuZGVuY2llc1wiOiB7XG4gICAgXCJAZW1uYXBpL3J1bnRpbWVcIjogXCJeMS43LjFcIixcbiAgICBcIkBveGMtbm9kZS9jb3JlXCI6IFwiXjAuMC4zNVwiLFxuICAgIFwiQHN0ZC90b21sXCI6IFwibnBtOkBqc3Ivc3RkX190b21sQF4xLjAuMTFcIixcbiAgICBcIkB0eXBlcy9pbnF1aXJlclwiOiBcIl45LjAuOVwiLFxuICAgIFwiQHR5cGVzL2pzLXlhbWxcIjogXCJeNC4wLjlcIixcbiAgICBcIkB0eXBlcy9ub2RlXCI6IFwiXjI0LjEwLjBcIixcbiAgICBcIkB0eXBlcy9zZW12ZXJcIjogXCJeNy43LjFcIixcbiAgICBcImF2YVwiOiBcIl42LjQuMVwiLFxuICAgIFwiZW1wYXRoaWNcIjogXCJeMi4wLjBcIixcbiAgICBcImVudi1wYXRoc1wiOiBcIl4zLjAuMFwiLFxuICAgIFwicHJldHRpZXJcIjogXCJeMy42LjJcIixcbiAgICBcInRzZG93blwiOiBcIl4wLjE4LjBcIixcbiAgICBcInRzbGliXCI6IFwiXjIuOC4xXCIsXG4gICAgXCJ0eXBlc2NyaXB0XCI6IFwiXjUuOS4zXCJcbiAgfSxcbiAgXCJwZWVyRGVwZW5kZW5jaWVzXCI6IHtcbiAgICBcIkBlbW5hcGkvcnVudGltZVwiOiBcIl4xLjcuMVwiXG4gIH0sXG4gIFwicGVlckRlcGVuZGVuY2llc01ldGFcIjoge1xuICAgIFwiQGVtbmFwaS9ydW50aW1lXCI6IHtcbiAgICAgIFwib3B0aW9uYWxcIjogdHJ1ZVxuICAgIH1cbiAgfSxcbiAgXCJmdW5kaW5nXCI6IHtcbiAgICBcInR5cGVcIjogXCJnaXRodWJcIixcbiAgICBcInVybFwiOiBcImh0dHBzOi8vZ2l0aHViLmNvbS9zcG9uc29ycy9Ccm9vb29vb2tseW5cIlxuICB9LFxuICBcInNjcmlwdHNcIjoge1xuICAgIFwiY29kZWdlblwiOiBcIm5vZGUgLS1pbXBvcnQgQG94Yy1ub2RlL2NvcmUvcmVnaXN0ZXIgLi9jb2RlZ2VuL2luZGV4LnRzXCIsXG4gICAgXCJidWlsZFwiOiBcInRzZG93blwiLFxuICAgIFwidGVzdFwiOiBcIm5vZGUgLS1pbXBvcnQgQG94Yy1ub2RlL2NvcmUvcmVnaXN0ZXIgLi4vbm9kZV9tb2R1bGVzL2F2YS9lbnRyeXBvaW50cy9jbGkubWpzXCJcbiAgfSxcbiAgXCJhdmFcIjoge1xuICAgIFwiZXh0ZW5zaW9uc1wiOiB7XG4gICAgICBcInRzXCI6IFwibW9kdWxlXCJcbiAgICB9LFxuICAgIFwidGltZW91dFwiOiBcIjJtXCIsXG4gICAgXCJ3b3JrZXJUaHJlYWRzXCI6IGZhbHNlLFxuICAgIFwiZmlsZXNcIjogW1xuICAgICAgXCIqKi9fX3Rlc3RzX18vKiovKi5zcGVjLnRzXCIsXG4gICAgICBcImUyZS8qKi8qLnNwZWMudHNcIlxuICAgIF1cbiAgfVxufVxuIiwiaW1wb3J0IHtcbiAgcmVhZEZpbGUsXG4gIHdyaXRlRmlsZSxcbiAgdW5saW5rLFxuICBjb3B5RmlsZSxcbiAgbWtkaXIsXG4gIHN0YXQsXG4gIHJlYWRkaXIsXG4gIGFjY2Vzcyxcbn0gZnJvbSAnbm9kZTpmcy9wcm9taXNlcydcblxuaW1wb3J0IHBrZ0pzb24gZnJvbSAnLi4vLi4vcGFja2FnZS5qc29uJyB3aXRoIHsgdHlwZTogJ2pzb24nIH1cbmltcG9ydCB7IGRlYnVnIH0gZnJvbSAnLi9sb2cuanMnXG5cbmV4cG9ydCBjb25zdCByZWFkRmlsZUFzeW5jID0gcmVhZEZpbGVcbmV4cG9ydCBjb25zdCB3cml0ZUZpbGVBc3luYyA9IHdyaXRlRmlsZVxuZXhwb3J0IGNvbnN0IHVubGlua0FzeW5jID0gdW5saW5rXG5leHBvcnQgY29uc3QgY29weUZpbGVBc3luYyA9IGNvcHlGaWxlXG5leHBvcnQgY29uc3QgbWtkaXJBc3luYyA9IG1rZGlyXG5leHBvcnQgY29uc3Qgc3RhdEFzeW5jID0gc3RhdFxuZXhwb3J0IGNvbnN0IHJlYWRkaXJBc3luYyA9IHJlYWRkaXJcblxuZXhwb3J0IGZ1bmN0aW9uIGZpbGVFeGlzdHMocGF0aDogc3RyaW5nKTogUHJvbWlzZTxib29sZWFuPiB7XG4gIHJldHVybiBhY2Nlc3MocGF0aCkudGhlbihcbiAgICAoKSA9PiB0cnVlLFxuICAgICgpID0+IGZhbHNlLFxuICApXG59XG5cbmV4cG9ydCBhc3luYyBmdW5jdGlvbiBkaXJFeGlzdHNBc3luYyhwYXRoOiBzdHJpbmcpIHtcbiAgdHJ5IHtcbiAgICBjb25zdCBzdGF0cyA9IGF3YWl0IHN0YXRBc3luYyhwYXRoKVxuICAgIHJldHVybiBzdGF0cy5pc0RpcmVjdG9yeSgpXG4gIH0gY2F0Y2gge1xuICAgIHJldHVybiBmYWxzZVxuICB9XG59XG5cbmV4cG9ydCBmdW5jdGlvbiBwaWNrPE8sIEsgZXh0ZW5kcyBrZXlvZiBPPihvOiBPLCAuLi5rZXlzOiBLW10pOiBQaWNrPE8sIEs+IHtcbiAgcmV0dXJuIGtleXMucmVkdWNlKChhY2MsIGtleSkgPT4ge1xuICAgIGFjY1trZXldID0gb1trZXldXG4gICAgcmV0dXJuIGFjY1xuICB9LCB7fSBhcyBPKVxufVxuXG5leHBvcnQgYXN5bmMgZnVuY3Rpb24gdXBkYXRlUGFja2FnZUpzb24oXG4gIHBhdGg6IHN0cmluZyxcbiAgcGFydGlhbDogUmVjb3JkPHN0cmluZywgYW55Pixcbikge1xuICBjb25zdCBleGlzdHMgPSBhd2FpdCBmaWxlRXhpc3RzKHBhdGgpXG4gIGlmICghZXhpc3RzKSB7XG4gICAgZGVidWcoYEZpbGUgbm90IGV4aXN0cyAke3BhdGh9YClcbiAgICByZXR1cm5cbiAgfVxuICBjb25zdCBvbGQgPSBKU09OLnBhcnNlKGF3YWl0IHJlYWRGaWxlQXN5bmMocGF0aCwgJ3V0ZjgnKSlcbiAgYXdhaXQgd3JpdGVGaWxlQXN5bmMocGF0aCwgSlNPTi5zdHJpbmdpZnkoeyAuLi5vbGQsIC4uLnBhcnRpYWwgfSwgbnVsbCwgMikpXG59XG5cbmV4cG9ydCBjb25zdCBDTElfVkVSU0lPTiA9IHBrZ0pzb24udmVyc2lvblxuIiwiaW1wb3J0IHsgZXhlY1N5bmMgfSBmcm9tICdub2RlOmNoaWxkX3Byb2Nlc3MnXG5cbmV4cG9ydCB0eXBlIFBsYXRmb3JtID0gTm9kZUpTLlBsYXRmb3JtIHwgJ3dhc20nIHwgJ3dhc2knIHwgJ29wZW5oYXJtb255J1xuXG5leHBvcnQgY29uc3QgVU5JVkVSU0FMX1RBUkdFVFMgPSB7XG4gICd1bml2ZXJzYWwtYXBwbGUtZGFyd2luJzogWydhYXJjaDY0LWFwcGxlLWRhcndpbicsICd4ODZfNjQtYXBwbGUtZGFyd2luJ10sXG59IGFzIGNvbnN0XG5cbmNvbnN0IFNVQl9TWVNURU1TID0gbmV3IFNldChbJ2FuZHJvaWQnLCAnb2hvcyddKVxuXG5leHBvcnQgY29uc3QgQVZBSUxBQkxFX1RBUkdFVFMgPSBbXG4gICdhYXJjaDY0LWFwcGxlLWRhcndpbicsXG4gICdhYXJjaDY0LWxpbnV4LWFuZHJvaWQnLFxuICAnYWFyY2g2NC11bmtub3duLWxpbnV4LWdudScsXG4gICdhYXJjaDY0LXVua25vd24tbGludXgtbXVzbCcsXG4gICdhYXJjaDY0LXVua25vd24tbGludXgtb2hvcycsXG4gICdhYXJjaDY0LXBjLXdpbmRvd3MtbXN2YycsXG4gICd4ODZfNjQtYXBwbGUtZGFyd2luJyxcbiAgJ3g4Nl82NC1wYy13aW5kb3dzLW1zdmMnLFxuICAneDg2XzY0LXBjLXdpbmRvd3MtZ251JyxcbiAgJ3g4Nl82NC11bmtub3duLWxpbnV4LWdudScsXG4gICd4ODZfNjQtdW5rbm93bi1saW51eC1tdXNsJyxcbiAgJ3g4Nl82NC11bmtub3duLWxpbnV4LW9ob3MnLFxuICAneDg2XzY0LXVua25vd24tZnJlZWJzZCcsXG4gICdpNjg2LXBjLXdpbmRvd3MtbXN2YycsXG4gICdhcm12Ny11bmtub3duLWxpbnV4LWdudWVhYmloZicsXG4gICdhcm12Ny11bmtub3duLWxpbnV4LW11c2xlYWJpaGYnLFxuICAnYXJtdjctbGludXgtYW5kcm9pZGVhYmknLFxuICAndW5pdmVyc2FsLWFwcGxlLWRhcndpbicsXG4gICdsb29uZ2FyY2g2NC11bmtub3duLWxpbnV4LWdudScsXG4gICdyaXNjdjY0Z2MtdW5rbm93bi1saW51eC1nbnUnLFxuICAncG93ZXJwYzY0bGUtdW5rbm93bi1saW51eC1nbnUnLFxuICAnczM5MHgtdW5rbm93bi1saW51eC1nbnUnLFxuICAnd2FzbTMyLXdhc2ktcHJldmlldzEtdGhyZWFkcycsXG4gICd3YXNtMzItd2FzaXAxLXRocmVhZHMnLFxuXSBhcyBjb25zdFxuXG5leHBvcnQgdHlwZSBUYXJnZXRUcmlwbGUgPSAodHlwZW9mIEFWQUlMQUJMRV9UQVJHRVRTKVtudW1iZXJdXG5cbmV4cG9ydCBjb25zdCBERUZBVUxUX1RBUkdFVFMgPSBbXG4gICd4ODZfNjQtYXBwbGUtZGFyd2luJyxcbiAgJ2FhcmNoNjQtYXBwbGUtZGFyd2luJyxcbiAgJ3g4Nl82NC1wYy13aW5kb3dzLW1zdmMnLFxuICAneDg2XzY0LXVua25vd24tbGludXgtZ251Jyxcbl0gYXMgY29uc3RcblxuZXhwb3J0IGNvbnN0IFRBUkdFVF9MSU5LRVI6IFJlY29yZDxzdHJpbmcsIHN0cmluZz4gPSB7XG4gICdhYXJjaDY0LXVua25vd24tbGludXgtbXVzbCc6ICdhYXJjaDY0LWxpbnV4LW11c2wtZ2NjJyxcbiAgLy8gVE9ETzogU3dpdGNoIHRvIGxvb25nYXJjaDY0LWxpbnV4LWdudS1nY2Mgd2hlbiBhdmFpbGFibGVcbiAgJ2xvb25nYXJjaDY0LXVua25vd24tbGludXgtZ251JzogJ2xvb25nYXJjaDY0LWxpbnV4LWdudS1nY2MtMTMnLFxuICAncmlzY3Y2NGdjLXVua25vd24tbGludXgtZ251JzogJ3Jpc2N2NjQtbGludXgtZ251LWdjYycsXG4gICdwb3dlcnBjNjRsZS11bmtub3duLWxpbnV4LWdudSc6ICdwb3dlcnBjNjRsZS1saW51eC1nbnUtZ2NjJyxcbiAgJ3MzOTB4LXVua25vd24tbGludXgtZ251JzogJ3MzOTB4LWxpbnV4LWdudS1nY2MnLFxufVxuXG4vLyBodHRwczovL25vZGVqcy5vcmcvYXBpL3Byb2Nlc3MuaHRtbCNwcm9jZXNzX3Byb2Nlc3NfYXJjaFxudHlwZSBOb2RlSlNBcmNoID1cbiAgfCAnYXJtJ1xuICB8ICdhcm02NCdcbiAgfCAnaWEzMidcbiAgfCAnbG9vbmc2NCdcbiAgfCAnbWlwcydcbiAgfCAnbWlwc2VsJ1xuICB8ICdwcGMnXG4gIHwgJ3BwYzY0J1xuICB8ICdyaXNjdjY0J1xuICB8ICdzMzkwJ1xuICB8ICdzMzkweCdcbiAgfCAneDMyJ1xuICB8ICd4NjQnXG4gIHwgJ3VuaXZlcnNhbCdcbiAgfCAnd2FzbTMyJ1xuXG5jb25zdCBDcHVUb05vZGVBcmNoOiBSZWNvcmQ8c3RyaW5nLCBOb2RlSlNBcmNoPiA9IHtcbiAgeDg2XzY0OiAneDY0JyxcbiAgYWFyY2g2NDogJ2FybTY0JyxcbiAgaTY4NjogJ2lhMzInLFxuICBhcm12NzogJ2FybScsXG4gIGxvb25nYXJjaDY0OiAnbG9vbmc2NCcsXG4gIHJpc2N2NjRnYzogJ3Jpc2N2NjQnLFxuICBwb3dlcnBjNjRsZTogJ3BwYzY0Jyxcbn1cblxuZXhwb3J0IGNvbnN0IE5vZGVBcmNoVG9DcHU6IFJlY29yZDxzdHJpbmcsIHN0cmluZz4gPSB7XG4gIHg2NDogJ3g4Nl82NCcsXG4gIGFybTY0OiAnYWFyY2g2NCcsXG4gIGlhMzI6ICdpNjg2JyxcbiAgYXJtOiAnYXJtdjcnLFxuICBsb29uZzY0OiAnbG9vbmdhcmNoNjQnLFxuICByaXNjdjY0OiAncmlzY3Y2NGdjJyxcbiAgcHBjNjQ6ICdwb3dlcnBjNjRsZScsXG59XG5cbmNvbnN0IFN5c1RvTm9kZVBsYXRmb3JtOiBSZWNvcmQ8c3RyaW5nLCBQbGF0Zm9ybT4gPSB7XG4gIGxpbnV4OiAnbGludXgnLFxuICBmcmVlYnNkOiAnZnJlZWJzZCcsXG4gIGRhcndpbjogJ2RhcndpbicsXG4gIHdpbmRvd3M6ICd3aW4zMicsXG4gIG9ob3M6ICdvcGVuaGFybW9ueScsXG59XG5cbmV4cG9ydCBjb25zdCBVbmlBcmNoc0J5UGxhdGZvcm06IFBhcnRpYWw8UmVjb3JkPFBsYXRmb3JtLCBOb2RlSlNBcmNoW10+PiA9IHtcbiAgZGFyd2luOiBbJ3g2NCcsICdhcm02NCddLFxufVxuXG5leHBvcnQgaW50ZXJmYWNlIFRhcmdldCB7XG4gIHRyaXBsZTogc3RyaW5nXG4gIHBsYXRmb3JtQXJjaEFCSTogc3RyaW5nXG4gIHBsYXRmb3JtOiBQbGF0Zm9ybVxuICBhcmNoOiBOb2RlSlNBcmNoXG4gIGFiaTogc3RyaW5nIHwgbnVsbFxufVxuXG4vKipcbiAqIEEgdHJpcGxlIGlzIGEgc3BlY2lmaWMgZm9ybWF0IGZvciBzcGVjaWZ5aW5nIGEgdGFyZ2V0IGFyY2hpdGVjdHVyZS5cbiAqIFRyaXBsZXMgbWF5IGJlIHJlZmVycmVkIHRvIGFzIGEgdGFyZ2V0IHRyaXBsZSB3aGljaCBpcyB0aGUgYXJjaGl0ZWN0dXJlIGZvciB0aGUgYXJ0aWZhY3QgcHJvZHVjZWQsIGFuZCB0aGUgaG9zdCB0cmlwbGUgd2hpY2ggaXMgdGhlIGFyY2hpdGVjdHVyZSB0aGF0IHRoZSBjb21waWxlciBpcyBydW5uaW5nIG9uLlxuICogVGhlIGdlbmVyYWwgZm9ybWF0IG9mIHRoZSB0cmlwbGUgaXMgYDxhcmNoPjxzdWI+LTx2ZW5kb3I+LTxzeXM+LTxhYmk+YCB3aGVyZTpcbiAqICAgLSBgYXJjaGAgPSBUaGUgYmFzZSBDUFUgYXJjaGl0ZWN0dXJlLCBmb3IgZXhhbXBsZSBgeDg2XzY0YCwgYGk2ODZgLCBgYXJtYCwgYHRodW1iYCwgYG1pcHNgLCBldGMuXG4gKiAgIC0gYHN1YmAgPSBUaGUgQ1BVIHN1Yi1hcmNoaXRlY3R1cmUsIGZvciBleGFtcGxlIGBhcm1gIGhhcyBgdjdgLCBgdjdzYCwgYHY1dGVgLCBldGMuXG4gKiAgIC0gYHZlbmRvcmAgPSBUaGUgdmVuZG9yLCBmb3IgZXhhbXBsZSBgdW5rbm93bmAsIGBhcHBsZWAsIGBwY2AsIGBudmlkaWFgLCBldGMuXG4gKiAgIC0gYHN5c2AgPSBUaGUgc3lzdGVtIG5hbWUsIGZvciBleGFtcGxlIGBsaW51eGAsIGB3aW5kb3dzYCwgYGRhcndpbmAsIGV0Yy4gbm9uZSBpcyB0eXBpY2FsbHkgdXNlZCBmb3IgYmFyZS1tZXRhbCB3aXRob3V0IGFuIE9TLlxuICogICAtIGBhYmlgID0gVGhlIEFCSSwgZm9yIGV4YW1wbGUgYGdudWAsIGBhbmRyb2lkYCwgYGVhYmlgLCBldGMuXG4gKi9cbmV4cG9ydCBmdW5jdGlvbiBwYXJzZVRyaXBsZShyYXdUcmlwbGU6IHN0cmluZyk6IFRhcmdldCB7XG4gIGlmIChcbiAgICByYXdUcmlwbGUgPT09ICd3YXNtMzItd2FzaScgfHxcbiAgICByYXdUcmlwbGUgPT09ICd3YXNtMzItd2FzaS1wcmV2aWV3MS10aHJlYWRzJyB8fFxuICAgIHJhd1RyaXBsZS5zdGFydHNXaXRoKCd3YXNtMzItd2FzaXAnKVxuICApIHtcbiAgICByZXR1cm4ge1xuICAgICAgdHJpcGxlOiByYXdUcmlwbGUsXG4gICAgICBwbGF0Zm9ybUFyY2hBQkk6ICd3YXNtMzItd2FzaScsXG4gICAgICBwbGF0Zm9ybTogJ3dhc2knLFxuICAgICAgYXJjaDogJ3dhc20zMicsXG4gICAgICBhYmk6ICd3YXNpJyxcbiAgICB9XG4gIH1cbiAgY29uc3QgdHJpcGxlID0gcmF3VHJpcGxlLmVuZHNXaXRoKCdlYWJpJylcbiAgICA/IGAke3Jhd1RyaXBsZS5zbGljZSgwLCAtNCl9LWVhYmlgXG4gICAgOiByYXdUcmlwbGVcbiAgY29uc3QgdHJpcGxlcyA9IHRyaXBsZS5zcGxpdCgnLScpXG4gIGxldCBjcHU6IHN0cmluZ1xuICBsZXQgc3lzOiBzdHJpbmdcbiAgbGV0IGFiaTogc3RyaW5nIHwgbnVsbCA9IG51bGxcbiAgaWYgKHRyaXBsZXMubGVuZ3RoID09PSAyKSB7XG4gICAgLy8gYWFyY2g2NC1mdWNoc2lhXG4gICAgLy8gXiBjcHUgICBeIHN5c1xuICAgIDtbY3B1LCBzeXNdID0gdHJpcGxlc1xuICB9IGVsc2Uge1xuICAgIC8vIGFhcmNoNjQtdW5rbm93bi1saW51eC1tdXNsXG4gICAgLy8gXiBjcHUgICBedmVuZG9yIF4gc3lzIF4gYWJpXG4gICAgLy8gYWFyY2g2NC1hcHBsZS1kYXJ3aW5cbiAgICAvLyBeIGNwdSAgICAgICAgIF4gc3lzICAoYWJpIGlzIE5vbmUpXG4gICAgO1tjcHUsICwgc3lzLCBhYmkgPSBudWxsXSA9IHRyaXBsZXNcbiAgfVxuXG4gIGlmIChhYmkgJiYgU1VCX1NZU1RFTVMuaGFzKGFiaSkpIHtcbiAgICBzeXMgPSBhYmlcbiAgICBhYmkgPSBudWxsXG4gIH1cbiAgY29uc3QgcGxhdGZvcm0gPSBTeXNUb05vZGVQbGF0Zm9ybVtzeXNdID8/IChzeXMgYXMgUGxhdGZvcm0pXG4gIGNvbnN0IGFyY2ggPSBDcHVUb05vZGVBcmNoW2NwdV0gPz8gKGNwdSBhcyBOb2RlSlNBcmNoKVxuXG4gIHJldHVybiB7XG4gICAgdHJpcGxlOiByYXdUcmlwbGUsXG4gICAgcGxhdGZvcm1BcmNoQUJJOiBhYmkgPyBgJHtwbGF0Zm9ybX0tJHthcmNofS0ke2FiaX1gIDogYCR7cGxhdGZvcm19LSR7YXJjaH1gLFxuICAgIHBsYXRmb3JtLFxuICAgIGFyY2gsXG4gICAgYWJpLFxuICB9XG59XG5cbmV4cG9ydCBmdW5jdGlvbiBnZXRTeXN0ZW1EZWZhdWx0VGFyZ2V0KCk6IFRhcmdldCB7XG4gIGNvbnN0IGhvc3QgPSBleGVjU3luYyhgcnVzdGMgLXZWYCwge1xuICAgIGVudjogcHJvY2Vzcy5lbnYsXG4gIH0pXG4gICAgLnRvU3RyaW5nKCd1dGY4JylcbiAgICAuc3BsaXQoJ1xcbicpXG4gICAgLmZpbmQoKGxpbmUpID0+IGxpbmUuc3RhcnRzV2l0aCgnaG9zdDogJykpXG4gIGNvbnN0IHRyaXBsZSA9IGhvc3Q/LnNsaWNlKCdob3N0OiAnLmxlbmd0aClcbiAgaWYgKCF0cmlwbGUpIHtcbiAgICB0aHJvdyBuZXcgVHlwZUVycm9yKGBDYW4gbm90IHBhcnNlIHRhcmdldCB0cmlwbGUgZnJvbSBob3N0YClcbiAgfVxuICByZXR1cm4gcGFyc2VUcmlwbGUodHJpcGxlKVxufVxuXG5leHBvcnQgZnVuY3Rpb24gZ2V0VGFyZ2V0TGlua2VyKHRhcmdldDogc3RyaW5nKTogc3RyaW5nIHwgdW5kZWZpbmVkIHtcbiAgcmV0dXJuIFRBUkdFVF9MSU5LRVJbdGFyZ2V0XVxufVxuXG5leHBvcnQgZnVuY3Rpb24gdGFyZ2V0VG9FbnZWYXIodGFyZ2V0OiBzdHJpbmcpOiBzdHJpbmcge1xuICByZXR1cm4gdGFyZ2V0LnJlcGxhY2UoLy0vZywgJ18nKS50b1VwcGVyQ2FzZSgpXG59XG4iLCJleHBvcnQgZW51bSBOYXBpVmVyc2lvbiB7XG4gIE5hcGkxID0gMSxcbiAgTmFwaTIsXG4gIE5hcGkzLFxuICBOYXBpNCxcbiAgTmFwaTUsXG4gIE5hcGk2LFxuICBOYXBpNyxcbiAgTmFwaTgsXG4gIE5hcGk5LFxufVxuXG4vLy8gYmVjYXVzZSBub2RlIHN1cHBvcnQgbmV3IG5hcGkgdmVyc2lvbiBpbiBzb21lIG1pbm9yIHZlcnNpb24gdXBkYXRlcywgc28gd2UgbWlnaHQgbWVldCBzdWNoIHNpdHVhdGlvbjpcbi8vLyBgbm9kZSB2MTAuMjAuMGAgc3VwcG9ydHMgYG5hcGk1YCBhbmQgYG5hcGk2YCwgYnV0IGBub2RlIHYxMi4wLjBgIG9ubHkgc3VwcG9ydCBgbmFwaTRgLFxuLy8vIGJ5IHdoaWNoLCB3ZSBjYW4gbm90IHRlbGwgZGlyZWN0bHkgbmFwaSB2ZXJzaW9uIHN1cHBvcnRsZXNzIGZyb20gbm9kZSB2ZXJzaW9uIGRpcmVjdGx5LlxuY29uc3QgTkFQSV9WRVJTSU9OX01BVFJJWCA9IG5ldyBNYXA8TmFwaVZlcnNpb24sIHN0cmluZz4oW1xuICBbTmFwaVZlcnNpb24uTmFwaTEsICc4LjYuMCB8IDkuMC4wIHwgMTAuMC4wJ10sXG4gIFtOYXBpVmVyc2lvbi5OYXBpMiwgJzguMTAuMCB8IDkuMy4wIHwgMTAuMC4wJ10sXG4gIFtOYXBpVmVyc2lvbi5OYXBpMywgJzYuMTQuMiB8IDguMTEuMiB8IDkuMTEuMCB8IDEwLjAuMCddLFxuICBbTmFwaVZlcnNpb24uTmFwaTQsICcxMC4xNi4wIHwgMTEuOC4wIHwgMTIuMC4wJ10sXG4gIFtOYXBpVmVyc2lvbi5OYXBpNSwgJzEwLjE3LjAgfCAxMi4xMS4wIHwgMTMuMC4wJ10sXG4gIFtOYXBpVmVyc2lvbi5OYXBpNiwgJzEwLjIwLjAgfCAxMi4xNy4wIHwgMTQuMC4wJ10sXG4gIFtOYXBpVmVyc2lvbi5OYXBpNywgJzEwLjIzLjAgfCAxMi4xOS4wIHwgMTQuMTIuMCB8IDE1LjAuMCddLFxuICBbTmFwaVZlcnNpb24uTmFwaTgsICcxMi4yMi4wIHwgMTQuMTcuMCB8IDE1LjEyLjAgfCAxNi4wLjAnXSxcbiAgW05hcGlWZXJzaW9uLk5hcGk5LCAnMTguMTcuMCB8IDIwLjMuMCB8IDIxLjEuMCddLFxuXSlcblxuaW50ZXJmYWNlIE5vZGVWZXJzaW9uIHtcbiAgbWFqb3I6IG51bWJlclxuICBtaW5vcjogbnVtYmVyXG4gIHBhdGNoOiBudW1iZXJcbn1cblxuZnVuY3Rpb24gcGFyc2VOb2RlVmVyc2lvbih2OiBzdHJpbmcpOiBOb2RlVmVyc2lvbiB7XG4gIGNvbnN0IG1hdGNoZXMgPSB2Lm1hdGNoKC92PyhbMC05XSspXFwuKFswLTldKylcXC4oWzAtOV0rKS9pKVxuXG4gIGlmICghbWF0Y2hlcykge1xuICAgIHRocm93IG5ldyBFcnJvcignVW5rbm93biBub2RlIHZlcnNpb24gbnVtYmVyOiAnICsgdilcbiAgfVxuXG4gIGNvbnN0IFssIG1ham9yLCBtaW5vciwgcGF0Y2hdID0gbWF0Y2hlc1xuXG4gIHJldHVybiB7XG4gICAgbWFqb3I6IHBhcnNlSW50KG1ham9yKSxcbiAgICBtaW5vcjogcGFyc2VJbnQobWlub3IpLFxuICAgIHBhdGNoOiBwYXJzZUludChwYXRjaCksXG4gIH1cbn1cblxuZnVuY3Rpb24gcmVxdWlyZWROb2RlVmVyc2lvbnMobmFwaVZlcnNpb246IE5hcGlWZXJzaW9uKTogTm9kZVZlcnNpb25bXSB7XG4gIGNvbnN0IHJlcXVpcmVtZW50ID0gTkFQSV9WRVJTSU9OX01BVFJJWC5nZXQobmFwaVZlcnNpb24pXG5cbiAgaWYgKCFyZXF1aXJlbWVudCkge1xuICAgIHJldHVybiBbcGFyc2VOb2RlVmVyc2lvbignMTAuMC4wJyldXG4gIH1cblxuICByZXR1cm4gcmVxdWlyZW1lbnQuc3BsaXQoJ3wnKS5tYXAocGFyc2VOb2RlVmVyc2lvbilcbn1cblxuZnVuY3Rpb24gdG9FbmdpbmVSZXF1aXJlbWVudCh2ZXJzaW9uczogTm9kZVZlcnNpb25bXSk6IHN0cmluZyB7XG4gIGNvbnN0IHJlcXVpcmVtZW50czogc3RyaW5nW10gPSBbXVxuICB2ZXJzaW9ucy5mb3JFYWNoKCh2LCBpKSA9PiB7XG4gICAgbGV0IHJlcSA9ICcnXG4gICAgaWYgKGkgIT09IDApIHtcbiAgICAgIGNvbnN0IGxhc3RWZXJzaW9uID0gdmVyc2lvbnNbaSAtIDFdXG4gICAgICByZXEgKz0gYDwgJHtsYXN0VmVyc2lvbi5tYWpvciArIDF9YFxuICAgIH1cblxuICAgIHJlcSArPSBgJHtpID09PSAwID8gJycgOiAnIHx8ICd9Pj0gJHt2Lm1ham9yfS4ke3YubWlub3J9LiR7di5wYXRjaH1gXG4gICAgcmVxdWlyZW1lbnRzLnB1c2gocmVxKVxuICB9KVxuXG4gIHJldHVybiByZXF1aXJlbWVudHMuam9pbignICcpXG59XG5cbmV4cG9ydCBmdW5jdGlvbiBuYXBpRW5naW5lUmVxdWlyZW1lbnQobmFwaVZlcnNpb246IE5hcGlWZXJzaW9uKTogc3RyaW5nIHtcbiAgcmV0dXJuIHRvRW5naW5lUmVxdWlyZW1lbnQocmVxdWlyZWROb2RlVmVyc2lvbnMobmFwaVZlcnNpb24pKVxufVxuIiwiaW1wb3J0IHsgc3Bhd24gfSBmcm9tICdub2RlOmNoaWxkX3Byb2Nlc3MnXG5pbXBvcnQgZnMgZnJvbSAnbm9kZTpmcydcblxuZXhwb3J0IHR5cGUgQ3JhdGVUYXJnZXRLaW5kID1cbiAgfCAnYmluJ1xuICB8ICdleGFtcGxlJ1xuICB8ICd0ZXN0J1xuICB8ICdiZW5jaCdcbiAgfCAnbGliJ1xuICB8ICdybGliJ1xuICB8ICdjZHlsaWInXG4gIHwgJ2N1c3RvbS1idWlsZCdcblxuZXhwb3J0IGludGVyZmFjZSBDcmF0ZVRhcmdldCB7XG4gIG5hbWU6IHN0cmluZ1xuICBraW5kOiBDcmF0ZVRhcmdldEtpbmRbXVxuICBjcmF0ZV90eXBlczogQ3JhdGVUYXJnZXRLaW5kW11cbn1cblxuZXhwb3J0IGludGVyZmFjZSBDcmF0ZSB7XG4gIGlkOiBzdHJpbmdcbiAgbmFtZTogc3RyaW5nXG4gIHNyY19wYXRoOiBzdHJpbmdcbiAgdmVyc2lvbjogc3RyaW5nXG4gIGVkaXRpb246IHN0cmluZ1xuICB0YXJnZXRzOiBDcmF0ZVRhcmdldFtdXG4gIGZlYXR1cmVzOiBSZWNvcmQ8c3RyaW5nLCBzdHJpbmdbXT5cbiAgbWFuaWZlc3RfcGF0aDogc3RyaW5nXG4gIGRlcGVuZGVuY2llczogQXJyYXk8e1xuICAgIG5hbWU6IHN0cmluZ1xuICAgIHNvdXJjZTogc3RyaW5nXG4gICAgcmVxOiBzdHJpbmdcbiAgICBraW5kOiBzdHJpbmcgfCBudWxsXG4gICAgcmVuYW1lOiBzdHJpbmcgfCBudWxsXG4gICAgb3B0aW9uYWw6IGJvb2xlYW5cbiAgICB1c2VzX2RlZmF1bHRfZmVhdHVyZXM6IGJvb2xlYW5cbiAgICBmZWF0dXJlczogc3RyaW5nW11cbiAgICB0YXJnZXQ6IHN0cmluZyB8IG51bGxcbiAgICByZWdpc3RyeTogc3RyaW5nIHwgbnVsbFxuICB9PlxufVxuXG5leHBvcnQgaW50ZXJmYWNlIENhcmdvV29ya3NwYWNlTWV0YWRhdGEge1xuICB2ZXJzaW9uOiBudW1iZXJcbiAgcGFja2FnZXM6IENyYXRlW11cbiAgd29ya3NwYWNlX21lbWJlcnM6IHN0cmluZ1tdXG4gIHRhcmdldF9kaXJlY3Rvcnk6IHN0cmluZ1xuICB3b3Jrc3BhY2Vfcm9vdDogc3RyaW5nXG59XG5cbmV4cG9ydCBhc3luYyBmdW5jdGlvbiBwYXJzZU1ldGFkYXRhKG1hbmlmZXN0UGF0aDogc3RyaW5nKSB7XG4gIGlmICghZnMuZXhpc3RzU3luYyhtYW5pZmVzdFBhdGgpKSB7XG4gICAgdGhyb3cgbmV3IEVycm9yKGBObyBjcmF0ZSBmb3VuZCBpbiBtYW5pZmVzdDogJHttYW5pZmVzdFBhdGh9YClcbiAgfVxuXG4gIGNvbnN0IGNoaWxkUHJvY2VzcyA9IHNwYXduKFxuICAgICdjYXJnbycsXG4gICAgWydtZXRhZGF0YScsICctLW1hbmlmZXN0LXBhdGgnLCBtYW5pZmVzdFBhdGgsICctLWZvcm1hdC12ZXJzaW9uJywgJzEnXSxcbiAgICB7IHN0ZGlvOiAncGlwZScgfSxcbiAgKVxuXG4gIGxldCBzdGRvdXQgPSAnJ1xuICBsZXQgc3RkZXJyID0gJydcbiAgbGV0IHN0YXR1cyA9IDBcbiAgbGV0IGVycm9yID0gbnVsbFxuXG4gIGNoaWxkUHJvY2Vzcy5zdGRvdXQub24oJ2RhdGEnLCAoZGF0YSkgPT4ge1xuICAgIHN0ZG91dCArPSBkYXRhXG4gIH0pXG5cbiAgY2hpbGRQcm9jZXNzLnN0ZGVyci5vbignZGF0YScsIChkYXRhKSA9PiB7XG4gICAgc3RkZXJyICs9IGRhdGFcbiAgfSlcblxuICBhd2FpdCBuZXcgUHJvbWlzZTx2b2lkPigocmVzb2x2ZSkgPT4ge1xuICAgIGNoaWxkUHJvY2Vzcy5vbignY2xvc2UnLCAoY29kZSkgPT4ge1xuICAgICAgc3RhdHVzID0gY29kZSA/PyAwXG4gICAgICByZXNvbHZlKClcbiAgICB9KVxuICB9KVxuXG4gIGlmIChlcnJvcikge1xuICAgIHRocm93IG5ldyBFcnJvcignY2FyZ28gbWV0YWRhdGEgZmFpbGVkIHRvIHJ1bicsIHsgY2F1c2U6IGVycm9yIH0pXG4gIH1cbiAgaWYgKHN0YXR1cyAhPT0gMCkge1xuICAgIGNvbnN0IHNpbXBsZU1lc3NhZ2UgPSBgY2FyZ28gbWV0YWRhdGEgZXhpdGVkIHdpdGggY29kZSAke3N0YXR1c31gXG4gICAgdGhyb3cgbmV3IEVycm9yKGAke3NpbXBsZU1lc3NhZ2V9IGFuZCBlcnJvciBtZXNzYWdlOlxcblxcbiR7c3RkZXJyfWAsIHtcbiAgICAgIGNhdXNlOiBuZXcgRXJyb3Ioc2ltcGxlTWVzc2FnZSksXG4gICAgfSlcbiAgfVxuXG4gIHRyeSB7XG4gICAgcmV0dXJuIEpTT04ucGFyc2Uoc3Rkb3V0KSBhcyBDYXJnb1dvcmtzcGFjZU1ldGFkYXRhXG4gIH0gY2F0Y2ggKGUpIHtcbiAgICB0aHJvdyBuZXcgRXJyb3IoJ0ZhaWxlZCB0byBwYXJzZSBjYXJnbyBtZXRhZGF0YSBKU09OJywgeyBjYXVzZTogZSB9KVxuICB9XG59XG4iLCJpbXBvcnQgeyB1bmRlcmxpbmUsIHllbGxvdyB9IGZyb20gJ2NvbG9yZXR0ZSdcbmltcG9ydCB7IG1lcmdlLCBvbWl0IH0gZnJvbSAnZXMtdG9vbGtpdCdcblxuaW1wb3J0IHsgZmlsZUV4aXN0cywgcmVhZEZpbGVBc3luYyB9IGZyb20gJy4vbWlzYy5qcydcbmltcG9ydCB7IERFRkFVTFRfVEFSR0VUUywgcGFyc2VUcmlwbGUsIHR5cGUgVGFyZ2V0IH0gZnJvbSAnLi90YXJnZXQuanMnXG5cbmV4cG9ydCB0eXBlIFZhbHVlT2ZDb25zdEFycmF5PFQ+ID0gVFtFeGNsdWRlPGtleW9mIFQsIGtleW9mIEFycmF5PGFueT4+XVxuXG5leHBvcnQgY29uc3QgU3VwcG9ydGVkUGFja2FnZU1hbmFnZXJzID0gWyd5YXJuJywgJ3BucG0nXSBhcyBjb25zdFxuZXhwb3J0IGNvbnN0IFN1cHBvcnRlZFRlc3RGcmFtZXdvcmtzID0gWydhdmEnXSBhcyBjb25zdFxuXG5leHBvcnQgdHlwZSBTdXBwb3J0ZWRQYWNrYWdlTWFuYWdlciA9IFZhbHVlT2ZDb25zdEFycmF5PFxuICB0eXBlb2YgU3VwcG9ydGVkUGFja2FnZU1hbmFnZXJzXG4+XG5leHBvcnQgdHlwZSBTdXBwb3J0ZWRUZXN0RnJhbWV3b3JrID0gVmFsdWVPZkNvbnN0QXJyYXk8XG4gIHR5cGVvZiBTdXBwb3J0ZWRUZXN0RnJhbWV3b3Jrc1xuPlxuXG5leHBvcnQgaW50ZXJmYWNlIFVzZXJOYXBpQ29uZmlnIHtcbiAgLyoqXG4gICAqIE5hbWUgb2YgdGhlIGJpbmFyeSB0byBiZSBnZW5lcmF0ZWQsIGRlZmF1bHQgdG8gYGluZGV4YFxuICAgKi9cbiAgYmluYXJ5TmFtZT86IHN0cmluZ1xuICAvKipcbiAgICogTmFtZSBvZiB0aGUgbnBtIHBhY2thZ2UsIGRlZmF1bHQgdG8gdGhlIG5hbWUgb2Ygcm9vdCBwYWNrYWdlLmpzb24gbmFtZVxuICAgKlxuICAgKiBBbHdheXMgZ2l2ZW4gYEBzY29wZS9wa2dgIGFuZCBhcmNoIHN1ZmZpeCB3aWxsIGJlIGFwcGVuZGVkIGxpa2UgYEBzY29wZS9wa2ctbGludXgtZ251LXg2NGBcbiAgICovXG4gIHBhY2thZ2VOYW1lPzogc3RyaW5nXG4gIC8qKlxuICAgKiBBbGwgdGFyZ2V0cyB0aGUgY3JhdGUgd2lsbCBiZSBjb21waWxlZCBmb3JcbiAgICovXG4gIHRhcmdldHM/OiBzdHJpbmdbXVxuXG4gIC8qKlxuICAgKiBUaGUgbnBtIGNsaWVudCBwcm9qZWN0IHVzZXMuXG4gICAqL1xuICBucG1DbGllbnQ/OiBzdHJpbmdcblxuICAvKipcbiAgICogV2hldGhlciBnZW5lcmF0ZSBjb25zdCBlbnVtIGZvciB0eXBlc2NyaXB0IGJpbmRpbmdzXG4gICAqL1xuICBjb25zdEVudW0/OiBib29sZWFuXG5cbiAgLyoqXG4gICAqIGR0cyBoZWFkZXIgcHJlcGVuZCB0byB0aGUgZ2VuZXJhdGVkIGR0cyBmaWxlXG4gICAqL1xuICBkdHNIZWFkZXI/OiBzdHJpbmdcblxuICAvKipcbiAgICogZHRzIGhlYWRlciBmaWxlIHBhdGggdG8gYmUgcHJlcGVuZGVkIHRvIHRoZSBnZW5lcmF0ZWQgZHRzIGZpbGVcbiAgICogaWYgYm90aCBkdHNIZWFkZXIgYW5kIGR0c0hlYWRlckZpbGUgYXJlIHByb3ZpZGVkLCBkdHNIZWFkZXJGaWxlIHdpbGwgYmUgdXNlZFxuICAgKi9cbiAgZHRzSGVhZGVyRmlsZT86IHN0cmluZ1xuXG4gIC8qKlxuICAgKiB3YXNtIGNvbXBpbGF0aW9uIG9wdGlvbnNcbiAgICovXG4gIHdhc20/OiB7XG4gICAgLyoqXG4gICAgICogaHR0cHM6Ly9kZXZlbG9wZXIubW96aWxsYS5vcmcvZW4tVVMvZG9jcy9XZWJBc3NlbWJseS9KYXZhU2NyaXB0X2ludGVyZmFjZS9NZW1vcnlcbiAgICAgKiBAZGVmYXVsdCA0MDAwIHBhZ2VzICgyNTZNaUIpXG4gICAgICovXG4gICAgaW5pdGlhbE1lbW9yeT86IG51bWJlclxuICAgIC8qKlxuICAgICAqIEBkZWZhdWx0IDY1NTM2IHBhZ2VzICg0R2lCKVxuICAgICAqL1xuICAgIG1heGltdW1NZW1vcnk/OiBudW1iZXJcblxuICAgIC8qKlxuICAgICAqIEJyb3dzZXIgd2FzbSBiaW5kaW5nIGNvbmZpZ3VyYXRpb25cbiAgICAgKi9cbiAgICBicm93c2VyOiB7XG4gICAgICAvKipcbiAgICAgICAqIFdoZXRoZXIgdG8gdXNlIGZzIG1vZHVsZSBpbiBicm93c2VyXG4gICAgICAgKi9cbiAgICAgIGZzPzogYm9vbGVhblxuICAgICAgLyoqXG4gICAgICAgKiBXaGV0aGVyIHRvIGluaXRpYWxpemUgd2FzbSBhc3luY2hyb25vdXNseVxuICAgICAgICovXG4gICAgICBhc3luY0luaXQ/OiBib29sZWFuXG4gICAgICAvKipcbiAgICAgICAqIFdoZXRoZXIgdG8gaW5qZWN0IGBidWZmZXJgIHRvIGVtbmFwaSBjb250ZXh0XG4gICAgICAgKi9cbiAgICAgIGJ1ZmZlcj86IGJvb2xlYW5cbiAgICB9XG4gIH1cblxuICAvKipcbiAgICogQGRlcHJlY2F0ZWQgYmluYXJ5TmFtZSBpbnN0ZWFkXG4gICAqL1xuICBuYW1lPzogc3RyaW5nXG4gIC8qKlxuICAgKiBAZGVwcmVjYXRlZCB1c2UgcGFja2FnZU5hbWUgaW5zdGVhZFxuICAgKi9cbiAgcGFja2FnZT86IHtcbiAgICBuYW1lPzogc3RyaW5nXG4gIH1cbiAgLyoqXG4gICAqIEBkZXByZWNhdGVkIHVzZSB0YXJnZXRzIGluc3RlYWRcbiAgICovXG4gIHRyaXBsZXM/OiB7XG4gICAgLyoqXG4gICAgICogV2hldGhlciBlbmFibGUgZGVmYXVsdCB0YXJnZXRzXG4gICAgICovXG4gICAgZGVmYXVsdHM6IGJvb2xlYW5cbiAgICAvKipcbiAgICAgKiBBZGRpdGlvbmFsIHRhcmdldHMgdG8gYmUgY29tcGlsZWQgZm9yXG4gICAgICovXG4gICAgYWRkaXRpb25hbD86IHN0cmluZ1tdXG4gIH1cbn1cblxuZXhwb3J0IGludGVyZmFjZSBDb21tb25QYWNrYWdlSnNvbkZpZWxkcyB7XG4gIG5hbWU6IHN0cmluZ1xuICB2ZXJzaW9uOiBzdHJpbmdcbiAgZGVzY3JpcHRpb24/OiBzdHJpbmdcbiAga2V5d29yZHM/OiBzdHJpbmdbXVxuICBhdXRob3I/OiBzdHJpbmdcbiAgYXV0aG9ycz86IHN0cmluZ1tdXG4gIGxpY2Vuc2U/OiBzdHJpbmdcbiAgY3B1Pzogc3RyaW5nW11cbiAgb3M/OiBzdHJpbmdbXVxuICBsaWJjPzogc3RyaW5nW11cbiAgZmlsZXM/OiBzdHJpbmdbXVxuICByZXBvc2l0b3J5PzogYW55XG4gIGhvbWVwYWdlPzogYW55XG4gIGVuZ2luZXM/OiBSZWNvcmQ8c3RyaW5nLCBzdHJpbmc+XG4gIHB1Ymxpc2hDb25maWc/OiBhbnlcbiAgYnVncz86IGFueVxuICAvLyBlc2xpbnQtZGlzYWJsZS1uZXh0LWxpbmUgbm8tdXNlLWJlZm9yZS1kZWZpbmVcbiAgbmFwaT86IFVzZXJOYXBpQ29uZmlnXG4gIHR5cGU/OiAnbW9kdWxlJyB8ICdjb21tb25qcydcbiAgc2NyaXB0cz86IFJlY29yZDxzdHJpbmcsIHN0cmluZz5cblxuICAvLyBtb2R1bGVzXG4gIG1haW4/OiBzdHJpbmdcbiAgbW9kdWxlPzogc3RyaW5nXG4gIHR5cGVzPzogc3RyaW5nXG4gIGJyb3dzZXI/OiBzdHJpbmdcbiAgZXhwb3J0cz86IGFueVxuXG4gIGRlcGVuZGVuY2llcz86IFJlY29yZDxzdHJpbmcsIHN0cmluZz5cbiAgZGV2RGVwZW5kZW5jaWVzPzogUmVjb3JkPHN0cmluZywgc3RyaW5nPlxuXG4gIGF2YT86IHtcbiAgICB0aW1lb3V0Pzogc3RyaW5nXG4gIH1cbn1cblxuZXhwb3J0IHR5cGUgTmFwaUNvbmZpZyA9IFJlcXVpcmVkPFxuICBQaWNrPFVzZXJOYXBpQ29uZmlnLCAnYmluYXJ5TmFtZScgfCAncGFja2FnZU5hbWUnIHwgJ25wbUNsaWVudCc+XG4+ICZcbiAgUGljazxVc2VyTmFwaUNvbmZpZywgJ3dhc20nIHwgJ2R0c0hlYWRlcicgfCAnZHRzSGVhZGVyRmlsZScgfCAnY29uc3RFbnVtJz4gJiB7XG4gICAgdGFyZ2V0czogVGFyZ2V0W11cbiAgICBwYWNrYWdlSnNvbjogQ29tbW9uUGFja2FnZUpzb25GaWVsZHNcbiAgfVxuXG5leHBvcnQgYXN5bmMgZnVuY3Rpb24gcmVhZE5hcGlDb25maWcoXG4gIHBhdGg6IHN0cmluZyxcbiAgY29uZmlnUGF0aD86IHN0cmluZyxcbik6IFByb21pc2U8TmFwaUNvbmZpZz4ge1xuICBpZiAoY29uZmlnUGF0aCAmJiAhKGF3YWl0IGZpbGVFeGlzdHMoY29uZmlnUGF0aCkpKSB7XG4gICAgdGhyb3cgbmV3IEVycm9yKGBOQVBJLVJTIGNvbmZpZyBub3QgZm91bmQgYXQgJHtjb25maWdQYXRofWApXG4gIH1cbiAgaWYgKCEoYXdhaXQgZmlsZUV4aXN0cyhwYXRoKSkpIHtcbiAgICB0aHJvdyBuZXcgRXJyb3IoYHBhY2thZ2UuanNvbiBub3QgZm91bmQgYXQgJHtwYXRofWApXG4gIH1cbiAgLy8gTWF5IHN1cHBvcnQgbXVsdGlwbGUgY29uZmlnIHNvdXJjZXMgbGF0ZXIgb24uXG4gIGNvbnN0IGNvbnRlbnQgPSBhd2FpdCByZWFkRmlsZUFzeW5jKHBhdGgsICd1dGY4JylcbiAgbGV0IHBrZ0pzb25cbiAgdHJ5IHtcbiAgICBwa2dKc29uID0gSlNPTi5wYXJzZShjb250ZW50KSBhcyBDb21tb25QYWNrYWdlSnNvbkZpZWxkc1xuICB9IGNhdGNoIChlKSB7XG4gICAgdGhyb3cgbmV3IEVycm9yKGBGYWlsZWQgdG8gcGFyc2UgcGFja2FnZS5qc29uIGF0ICR7cGF0aH1gLCB7XG4gICAgICBjYXVzZTogZSxcbiAgICB9KVxuICB9XG5cbiAgbGV0IHNlcGFyYXRlZENvbmZpZzogVXNlck5hcGlDb25maWcgfCB1bmRlZmluZWRcbiAgaWYgKGNvbmZpZ1BhdGgpIHtcbiAgICBjb25zdCBjb25maWdDb250ZW50ID0gYXdhaXQgcmVhZEZpbGVBc3luYyhjb25maWdQYXRoLCAndXRmOCcpXG4gICAgdHJ5IHtcbiAgICAgIHNlcGFyYXRlZENvbmZpZyA9IEpTT04ucGFyc2UoY29uZmlnQ29udGVudCkgYXMgVXNlck5hcGlDb25maWdcbiAgICB9IGNhdGNoIChlKSB7XG4gICAgICB0aHJvdyBuZXcgRXJyb3IoYEZhaWxlZCB0byBwYXJzZSBOQVBJLVJTIGNvbmZpZyBhdCAke2NvbmZpZ1BhdGh9YCwge1xuICAgICAgICBjYXVzZTogZSxcbiAgICAgIH0pXG4gICAgfVxuICB9XG5cbiAgY29uc3QgdXNlck5hcGlDb25maWcgPSBwa2dKc29uLm5hcGkgPz8ge31cbiAgaWYgKHBrZ0pzb24ubmFwaSAmJiBzZXBhcmF0ZWRDb25maWcpIHtcbiAgICBjb25zdCBwa2dKc29uUGF0aCA9IHVuZGVybGluZShwYXRoKVxuICAgIGNvbnN0IGNvbmZpZ1BhdGhVbmRlcmxpbmUgPSB1bmRlcmxpbmUoY29uZmlnUGF0aCEpXG4gICAgY29uc29sZS53YXJuKFxuICAgICAgeWVsbG93KFxuICAgICAgICBgQm90aCBuYXBpIGZpZWxkIGluICR7cGtnSnNvblBhdGh9IGFuZCBbTkFQSS1SUyBjb25maWddKCR7Y29uZmlnUGF0aFVuZGVybGluZX0pIGZpbGUgYXJlIGZvdW5kLCB0aGUgTkFQSS1SUyBjb25maWcgZmlsZSB3aWxsIGJlIHVzZWQuYCxcbiAgICAgICksXG4gICAgKVxuICB9XG4gIGlmIChzZXBhcmF0ZWRDb25maWcpIHtcbiAgICBPYmplY3QuYXNzaWduKHVzZXJOYXBpQ29uZmlnLCBzZXBhcmF0ZWRDb25maWcpXG4gIH1cbiAgY29uc3QgbmFwaUNvbmZpZzogTmFwaUNvbmZpZyA9IG1lcmdlKFxuICAgIHtcbiAgICAgIGJpbmFyeU5hbWU6ICdpbmRleCcsXG4gICAgICBwYWNrYWdlTmFtZTogcGtnSnNvbi5uYW1lLFxuICAgICAgdGFyZ2V0czogW10sXG4gICAgICBwYWNrYWdlSnNvbjogcGtnSnNvbixcbiAgICAgIG5wbUNsaWVudDogJ25wbScsXG4gICAgfSxcbiAgICBvbWl0KHVzZXJOYXBpQ29uZmlnLCBbJ3RhcmdldHMnXSksXG4gIClcblxuICBsZXQgdGFyZ2V0czogc3RyaW5nW10gPSB1c2VyTmFwaUNvbmZpZy50YXJnZXRzID8/IFtdXG5cbiAgLy8gY29tcGF0aWJsZSB3aXRoIG9sZCBjb25maWdcbiAgaWYgKHVzZXJOYXBpQ29uZmlnPy5uYW1lKSB7XG4gICAgY29uc29sZS53YXJuKFxuICAgICAgeWVsbG93KFxuICAgICAgICBgW0RFUFJFQ0FURURdIG5hcGkubmFtZSBpcyBkZXByZWNhdGVkLCB1c2UgbmFwaS5iaW5hcnlOYW1lIGluc3RlYWQuYCxcbiAgICAgICksXG4gICAgKVxuICAgIG5hcGlDb25maWcuYmluYXJ5TmFtZSA9IHVzZXJOYXBpQ29uZmlnLm5hbWVcbiAgfVxuXG4gIGlmICghdGFyZ2V0cy5sZW5ndGgpIHtcbiAgICBsZXQgZGVwcmVjYXRlZFdhcm5lZCA9IGZhbHNlXG4gICAgY29uc3Qgd2FybmluZyA9IHllbGxvdyhcbiAgICAgIGBbREVQUkVDQVRFRF0gbmFwaS50cmlwbGVzIGlzIGRlcHJlY2F0ZWQsIHVzZSBuYXBpLnRhcmdldHMgaW5zdGVhZC5gLFxuICAgIClcbiAgICBpZiAodXNlck5hcGlDb25maWcudHJpcGxlcz8uZGVmYXVsdHMpIHtcbiAgICAgIGRlcHJlY2F0ZWRXYXJuZWQgPSB0cnVlXG4gICAgICBjb25zb2xlLndhcm4od2FybmluZylcbiAgICAgIHRhcmdldHMgPSB0YXJnZXRzLmNvbmNhdChERUZBVUxUX1RBUkdFVFMpXG4gICAgfVxuXG4gICAgaWYgKHVzZXJOYXBpQ29uZmlnLnRyaXBsZXM/LmFkZGl0aW9uYWw/Lmxlbmd0aCkge1xuICAgICAgdGFyZ2V0cyA9IHRhcmdldHMuY29uY2F0KHVzZXJOYXBpQ29uZmlnLnRyaXBsZXMuYWRkaXRpb25hbClcbiAgICAgIGlmICghZGVwcmVjYXRlZFdhcm5lZCkge1xuICAgICAgICBjb25zb2xlLndhcm4od2FybmluZylcbiAgICAgIH1cbiAgICB9XG4gIH1cblxuICAvLyBmaW5kIGR1cGxpY2F0ZSB0YXJnZXRzXG4gIGNvbnN0IHVuaXF1ZVRhcmdldHMgPSBuZXcgU2V0KHRhcmdldHMpXG4gIGlmICh1bmlxdWVUYXJnZXRzLnNpemUgIT09IHRhcmdldHMubGVuZ3RoKSB7XG4gICAgY29uc3QgZHVwbGljYXRlVGFyZ2V0ID0gdGFyZ2V0cy5maW5kKFxuICAgICAgKHRhcmdldCwgaW5kZXgpID0+IHRhcmdldHMuaW5kZXhPZih0YXJnZXQpICE9PSBpbmRleCxcbiAgICApXG4gICAgdGhyb3cgbmV3IEVycm9yKGBEdXBsaWNhdGUgdGFyZ2V0cyBhcmUgbm90IGFsbG93ZWQ6ICR7ZHVwbGljYXRlVGFyZ2V0fWApXG4gIH1cblxuICBuYXBpQ29uZmlnLnRhcmdldHMgPSB0YXJnZXRzLm1hcChwYXJzZVRyaXBsZSlcblxuICByZXR1cm4gbmFwaUNvbmZpZ1xufVxuIiwiaW1wb3J0IHsgZXhlY1N5bmMgfSBmcm9tICdub2RlOmNoaWxkX3Byb2Nlc3MnXG5cbmltcG9ydCB7IGRlYnVnIH0gZnJvbSAnLi9sb2cuanMnXG5cbmV4cG9ydCBmdW5jdGlvbiB0cnlJbnN0YWxsQ2FyZ29CaW5hcnkobmFtZTogc3RyaW5nLCBiaW46IHN0cmluZykge1xuICBpZiAoZGV0ZWN0Q2FyZ29CaW5hcnkoYmluKSkge1xuICAgIGRlYnVnKCdDYXJnbyBiaW5hcnkgYWxyZWFkeSBpbnN0YWxsZWQ6ICVzJywgbmFtZSlcbiAgICByZXR1cm5cbiAgfVxuXG4gIHRyeSB7XG4gICAgZGVidWcoJ0luc3RhbGxpbmcgY2FyZ28gYmluYXJ5OiAlcycsIG5hbWUpXG4gICAgZXhlY1N5bmMoYGNhcmdvIGluc3RhbGwgJHtuYW1lfWAsIHtcbiAgICAgIHN0ZGlvOiAnaW5oZXJpdCcsXG4gICAgfSlcbiAgfSBjYXRjaCAoZSkge1xuICAgIHRocm93IG5ldyBFcnJvcihgRmFpbGVkIHRvIGluc3RhbGwgY2FyZ28gYmluYXJ5OiAke25hbWV9YCwge1xuICAgICAgY2F1c2U6IGUsXG4gICAgfSlcbiAgfVxufVxuXG5mdW5jdGlvbiBkZXRlY3RDYXJnb0JpbmFyeShiaW46IHN0cmluZykge1xuICBkZWJ1ZygnRGV0ZWN0aW5nIGNhcmdvIGJpbmFyeTogJXMnLCBiaW4pXG4gIHRyeSB7XG4gICAgZXhlY1N5bmMoYGNhcmdvIGhlbHAgJHtiaW59YCwge1xuICAgICAgc3RkaW86ICdpZ25vcmUnLFxuICAgIH0pXG4gICAgZGVidWcoJ0NhcmdvIGJpbmFyeSBkZXRlY3RlZDogJXMnLCBiaW4pXG4gICAgcmV0dXJuIHRydWVcbiAgfSBjYXRjaCB7XG4gICAgZGVidWcoJ0NhcmdvIGJpbmFyeSBub3QgZGV0ZWN0ZWQ6ICVzJywgYmluKVxuICAgIHJldHVybiBmYWxzZVxuICB9XG59XG4iLCJpbXBvcnQgeyBzb3J0QnkgfSBmcm9tICdlcy10b29sa2l0J1xuXG5pbXBvcnQgeyByZWFkRmlsZUFzeW5jIH0gZnJvbSAnLi9taXNjLmpzJ1xuXG5jb25zdCBUT1BfTEVWRUxfTkFNRVNQQUNFID0gJ19fVE9QX0xFVkVMX01PRFVMRV9fJ1xuZXhwb3J0IGNvbnN0IERFRkFVTFRfVFlQRV9ERUZfSEVBREVSID0gYC8qIGF1dG8tZ2VuZXJhdGVkIGJ5IE5BUEktUlMgKi9cbi8qIGVzbGludC1kaXNhYmxlICovXG5gXG5cbmVudW0gVHlwZURlZktpbmQge1xuICBDb25zdCA9ICdjb25zdCcsXG4gIEVudW0gPSAnZW51bScsXG4gIFN0cmluZ0VudW0gPSAnc3RyaW5nX2VudW0nLFxuICBJbnRlcmZhY2UgPSAnaW50ZXJmYWNlJyxcbiAgVHlwZSA9ICd0eXBlJyxcbiAgRm4gPSAnZm4nLFxuICBTdHJ1Y3QgPSAnc3RydWN0JyxcbiAgRXh0ZW5kcyA9ICdleHRlbmRzJyxcbiAgSW1wbCA9ICdpbXBsJyxcbn1cblxuaW50ZXJmYWNlIFR5cGVEZWZMaW5lIHtcbiAga2luZDogVHlwZURlZktpbmRcbiAgbmFtZTogc3RyaW5nXG4gIG9yaWdpbmFsX25hbWU/OiBzdHJpbmdcbiAgZGVmOiBzdHJpbmdcbiAgZXh0ZW5kcz86IHN0cmluZ1xuICBqc19kb2M/OiBzdHJpbmdcbiAganNfbW9kPzogc3RyaW5nXG59XG5cbmZ1bmN0aW9uIHByZXR0eVByaW50KFxuICBsaW5lOiBUeXBlRGVmTGluZSxcbiAgY29uc3RFbnVtOiBib29sZWFuLFxuICBpZGVudDogbnVtYmVyLFxuICBhbWJpZW50ID0gZmFsc2UsXG4pOiBzdHJpbmcge1xuICBsZXQgcyA9IGxpbmUuanNfZG9jID8/ICcnXG4gIHN3aXRjaCAobGluZS5raW5kKSB7XG4gICAgY2FzZSBUeXBlRGVmS2luZC5JbnRlcmZhY2U6XG4gICAgICBzICs9IGBleHBvcnQgaW50ZXJmYWNlICR7bGluZS5uYW1lfSB7XFxuJHtsaW5lLmRlZn1cXG59YFxuICAgICAgYnJlYWtcblxuICAgIGNhc2UgVHlwZURlZktpbmQuVHlwZTpcbiAgICAgIHMgKz0gYGV4cG9ydCB0eXBlICR7bGluZS5uYW1lfSA9IFxcbiR7bGluZS5kZWZ9YFxuICAgICAgYnJlYWtcblxuICAgIGNhc2UgVHlwZURlZktpbmQuRW51bTpcbiAgICAgIGNvbnN0IGVudW1OYW1lID0gY29uc3RFbnVtID8gJ2NvbnN0IGVudW0nIDogJ2VudW0nXG4gICAgICBzICs9IGAke2V4cG9ydERlY2xhcmUoYW1iaWVudCl9ICR7ZW51bU5hbWV9ICR7bGluZS5uYW1lfSB7XFxuJHtsaW5lLmRlZn1cXG59YFxuICAgICAgYnJlYWtcblxuICAgIGNhc2UgVHlwZURlZktpbmQuU3RyaW5nRW51bTpcbiAgICAgIGlmIChjb25zdEVudW0pIHtcbiAgICAgICAgcyArPSBgJHtleHBvcnREZWNsYXJlKGFtYmllbnQpfSBjb25zdCBlbnVtICR7bGluZS5uYW1lfSB7XFxuJHtsaW5lLmRlZn1cXG59YFxuICAgICAgfSBlbHNlIHtcbiAgICAgICAgcyArPSBgZXhwb3J0IHR5cGUgJHtsaW5lLm5hbWV9ID0gJHtsaW5lLmRlZi5yZXBsYWNlQWxsKC8uKj0vZywgJycpLnJlcGxhY2VBbGwoJywnLCAnfCcpfTtgXG4gICAgICB9XG4gICAgICBicmVha1xuXG4gICAgY2FzZSBUeXBlRGVmS2luZC5TdHJ1Y3Q6XG4gICAgICBjb25zdCBleHRlbmRzRGVmID0gbGluZS5leHRlbmRzID8gYCBleHRlbmRzICR7bGluZS5leHRlbmRzfWAgOiAnJ1xuICAgICAgaWYgKGxpbmUuZXh0ZW5kcykge1xuICAgICAgICAvLyBFeHRyYWN0IGdlbmVyaWMgcGFyYW1zIGZyb20gZXh0ZW5kcyB0eXBlIGxpa2UgSXRlcmF0b3I8VCwgVFJlc3VsdCwgVE5leHQ+XG4gICAgICAgIGNvbnN0IGdlbmVyaWNNYXRjaCA9IGxpbmUuZXh0ZW5kcy5tYXRjaCgvSXRlcmF0b3I8KC4rKT4kLylcbiAgICAgICAgaWYgKGdlbmVyaWNNYXRjaCkge1xuICAgICAgICAgIGNvbnN0IFtULCBUUmVzdWx0LCBUTmV4dF0gPSBnZW5lcmljTWF0Y2hbMV1cbiAgICAgICAgICAgIC5zcGxpdCgnLCcpXG4gICAgICAgICAgICAubWFwKChwKSA9PiBwLnRyaW0oKSlcbiAgICAgICAgICBsaW5lLmRlZiA9XG4gICAgICAgICAgICBsaW5lLmRlZiArXG4gICAgICAgICAgICBgXFxubmV4dCh2YWx1ZT86ICR7VE5leHR9KTogSXRlcmF0b3JSZXN1bHQ8JHtUfSwgJHtUUmVzdWx0fT5gXG4gICAgICAgIH1cbiAgICAgIH1cbiAgICAgIHMgKz0gYCR7ZXhwb3J0RGVjbGFyZShhbWJpZW50KX0gY2xhc3MgJHtsaW5lLm5hbWV9JHtleHRlbmRzRGVmfSB7XFxuJHtsaW5lLmRlZn1cXG59YFxuICAgICAgaWYgKGxpbmUub3JpZ2luYWxfbmFtZSAmJiBsaW5lLm9yaWdpbmFsX25hbWUgIT09IGxpbmUubmFtZSkge1xuICAgICAgICBzICs9IGBcXG5leHBvcnQgdHlwZSAke2xpbmUub3JpZ2luYWxfbmFtZX0gPSAke2xpbmUubmFtZX1gXG4gICAgICB9XG4gICAgICBicmVha1xuXG4gICAgY2FzZSBUeXBlRGVmS2luZC5GbjpcbiAgICAgIHMgKz0gYCR7ZXhwb3J0RGVjbGFyZShhbWJpZW50KX0gJHtsaW5lLmRlZn1gXG4gICAgICBicmVha1xuXG4gICAgZGVmYXVsdDpcbiAgICAgIHMgKz0gbGluZS5kZWZcbiAgfVxuXG4gIHJldHVybiBjb3JyZWN0U3RyaW5nSWRlbnQocywgaWRlbnQpXG59XG5cbmZ1bmN0aW9uIGV4cG9ydERlY2xhcmUoYW1iaWVudDogYm9vbGVhbik6IHN0cmluZyB7XG4gIGlmIChhbWJpZW50KSB7XG4gICAgcmV0dXJuICdleHBvcnQnXG4gIH1cblxuICByZXR1cm4gJ2V4cG9ydCBkZWNsYXJlJ1xufVxuXG5leHBvcnQgYXN5bmMgZnVuY3Rpb24gcHJvY2Vzc1R5cGVEZWYoXG4gIGludGVybWVkaWF0ZVR5cGVGaWxlOiBzdHJpbmcsXG4gIGNvbnN0RW51bTogYm9vbGVhbixcbikge1xuICBjb25zdCBleHBvcnRzOiBzdHJpbmdbXSA9IFtdXG4gIGNvbnN0IGRlZnMgPSBhd2FpdCByZWFkSW50ZXJtZWRpYXRlVHlwZUZpbGUoaW50ZXJtZWRpYXRlVHlwZUZpbGUpXG4gIGNvbnN0IGdyb3VwZWREZWZzID0gcHJlcHJvY2Vzc1R5cGVEZWYoZGVmcylcblxuICBjb25zdCBkdHMgPVxuICAgIHNvcnRCeShBcnJheS5mcm9tKGdyb3VwZWREZWZzKSwgWyhbbmFtZXNwYWNlXSkgPT4gbmFtZXNwYWNlXSlcbiAgICAgIC5tYXAoKFtuYW1lc3BhY2UsIGRlZnNdKSA9PiB7XG4gICAgICAgIGlmIChuYW1lc3BhY2UgPT09IFRPUF9MRVZFTF9OQU1FU1BBQ0UpIHtcbiAgICAgICAgICByZXR1cm4gZGVmc1xuICAgICAgICAgICAgLm1hcCgoZGVmKSA9PiB7XG4gICAgICAgICAgICAgIHN3aXRjaCAoZGVmLmtpbmQpIHtcbiAgICAgICAgICAgICAgICBjYXNlIFR5cGVEZWZLaW5kLkNvbnN0OlxuICAgICAgICAgICAgICAgIGNhc2UgVHlwZURlZktpbmQuRW51bTpcbiAgICAgICAgICAgICAgICBjYXNlIFR5cGVEZWZLaW5kLlN0cmluZ0VudW06XG4gICAgICAgICAgICAgICAgY2FzZSBUeXBlRGVmS2luZC5GbjpcbiAgICAgICAgICAgICAgICBjYXNlIFR5cGVEZWZLaW5kLlN0cnVjdDoge1xuICAgICAgICAgICAgICAgICAgZXhwb3J0cy5wdXNoKGRlZi5uYW1lKVxuICAgICAgICAgICAgICAgICAgaWYgKGRlZi5vcmlnaW5hbF9uYW1lICYmIGRlZi5vcmlnaW5hbF9uYW1lICE9PSBkZWYubmFtZSkge1xuICAgICAgICAgICAgICAgICAgICBleHBvcnRzLnB1c2goZGVmLm9yaWdpbmFsX25hbWUpXG4gICAgICAgICAgICAgICAgICB9XG4gICAgICAgICAgICAgICAgICBicmVha1xuICAgICAgICAgICAgICAgIH1cbiAgICAgICAgICAgICAgICBkZWZhdWx0OlxuICAgICAgICAgICAgICAgICAgYnJlYWtcbiAgICAgICAgICAgICAgfVxuICAgICAgICAgICAgICByZXR1cm4gcHJldHR5UHJpbnQoZGVmLCBjb25zdEVudW0sIDApXG4gICAgICAgICAgICB9KVxuICAgICAgICAgICAgLmpvaW4oJ1xcblxcbicpXG4gICAgICAgIH0gZWxzZSB7XG4gICAgICAgICAgZXhwb3J0cy5wdXNoKG5hbWVzcGFjZSlcbiAgICAgICAgICBsZXQgZGVjbGFyYXRpb24gPSAnJ1xuICAgICAgICAgIGRlY2xhcmF0aW9uICs9IGBleHBvcnQgZGVjbGFyZSBuYW1lc3BhY2UgJHtuYW1lc3BhY2V9IHtcXG5gXG4gICAgICAgICAgZm9yIChjb25zdCBkZWYgb2YgZGVmcykge1xuICAgICAgICAgICAgZGVjbGFyYXRpb24gKz0gcHJldHR5UHJpbnQoZGVmLCBjb25zdEVudW0sIDIsIHRydWUpICsgJ1xcbidcbiAgICAgICAgICB9XG4gICAgICAgICAgZGVjbGFyYXRpb24gKz0gJ30nXG4gICAgICAgICAgcmV0dXJuIGRlY2xhcmF0aW9uXG4gICAgICAgIH1cbiAgICAgIH0pXG4gICAgICAuam9pbignXFxuXFxuJykgKyAnXFxuJ1xuXG4gIHJldHVybiB7XG4gICAgZHRzLFxuICAgIGV4cG9ydHMsXG4gIH1cbn1cblxuYXN5bmMgZnVuY3Rpb24gcmVhZEludGVybWVkaWF0ZVR5cGVGaWxlKGZpbGU6IHN0cmluZykge1xuICBjb25zdCBjb250ZW50ID0gYXdhaXQgcmVhZEZpbGVBc3luYyhmaWxlLCAndXRmOCcpXG5cbiAgY29uc3QgZGVmcyA9IGNvbnRlbnRcbiAgICAuc3BsaXQoJ1xcbicpXG4gICAgLmZpbHRlcihCb29sZWFuKVxuICAgIC5tYXAoKGxpbmUpID0+IHtcbiAgICAgIGxpbmUgPSBsaW5lLnRyaW0oKVxuICAgICAgY29uc3QgcGFyc2VkID0gSlNPTi5wYXJzZShsaW5lKSBhcyBUeXBlRGVmTGluZVxuICAgICAgLy8gQ29udmVydCBlc2NhcGVkIG5ld2xpbmVzIGJhY2sgdG8gYWN0dWFsIG5ld2xpbmVzIGluIGpzX2RvYyBmaWVsZHNcbiAgICAgIGlmIChwYXJzZWQuanNfZG9jKSB7XG4gICAgICAgIHBhcnNlZC5qc19kb2MgPSBwYXJzZWQuanNfZG9jLnJlcGxhY2UoL1xcXFxuL2csICdcXG4nKVxuICAgICAgfVxuICAgICAgLy8gQ29udmVydCBlc2NhcGVkIG5ld2xpbmVzIHRvIGFjdHVhbCBuZXdsaW5lcyBpbiBkZWYgZmllbGRzIGZvciBzdHJ1Y3QvY2xhc3MvaW50ZXJmYWNlL3R5cGUgdHlwZXNcbiAgICAgIC8vIHdoZXJlIFxcbiByZXByZXNlbnRzIG1ldGhvZC9maWVsZCBzZXBhcmF0b3JzIHRoYXQgc2hvdWxkIGJlIGFjdHVhbCBuZXdsaW5lc1xuICAgICAgaWYgKHBhcnNlZC5kZWYpIHtcbiAgICAgICAgcGFyc2VkLmRlZiA9IHBhcnNlZC5kZWYucmVwbGFjZSgvXFxcXG4vZywgJ1xcbicpXG4gICAgICB9XG4gICAgICByZXR1cm4gcGFyc2VkXG4gICAgfSlcblxuICAvLyBtb3ZlIGFsbCBgc3RydWN0YCBkZWYgdG8gdGhlIHZlcnkgdG9wXG4gIC8vIGFuZCBvcmRlciB0aGUgcmVzdCBhbHBoYWJldGljYWxseS5cbiAgcmV0dXJuIGRlZnMuc29ydCgoYSwgYikgPT4ge1xuICAgIGlmIChhLmtpbmQgPT09IFR5cGVEZWZLaW5kLlN0cnVjdCkge1xuICAgICAgaWYgKGIua2luZCA9PT0gVHlwZURlZktpbmQuU3RydWN0KSB7XG4gICAgICAgIHJldHVybiBhLm5hbWUubG9jYWxlQ29tcGFyZShiLm5hbWUpXG4gICAgICB9XG4gICAgICByZXR1cm4gLTFcbiAgICB9IGVsc2UgaWYgKGIua2luZCA9PT0gVHlwZURlZktpbmQuU3RydWN0KSB7XG4gICAgICByZXR1cm4gMVxuICAgIH0gZWxzZSB7XG4gICAgICByZXR1cm4gYS5uYW1lLmxvY2FsZUNvbXBhcmUoYi5uYW1lKVxuICAgIH1cbiAgfSlcbn1cblxuZnVuY3Rpb24gcHJlcHJvY2Vzc1R5cGVEZWYoZGVmczogVHlwZURlZkxpbmVbXSk6IE1hcDxzdHJpbmcsIFR5cGVEZWZMaW5lW10+IHtcbiAgY29uc3QgbmFtZXNwYWNlR3JvdXBlZCA9IG5ldyBNYXA8c3RyaW5nLCBUeXBlRGVmTGluZVtdPigpXG4gIGNvbnN0IGNsYXNzRGVmcyA9IG5ldyBNYXA8c3RyaW5nLCBUeXBlRGVmTGluZT4oKVxuXG4gIGZvciAoY29uc3QgZGVmIG9mIGRlZnMpIHtcbiAgICBjb25zdCBuYW1lc3BhY2UgPSBkZWYuanNfbW9kID8/IFRPUF9MRVZFTF9OQU1FU1BBQ0VcbiAgICBpZiAoIW5hbWVzcGFjZUdyb3VwZWQuaGFzKG5hbWVzcGFjZSkpIHtcbiAgICAgIG5hbWVzcGFjZUdyb3VwZWQuc2V0KG5hbWVzcGFjZSwgW10pXG4gICAgfVxuXG4gICAgY29uc3QgZ3JvdXAgPSBuYW1lc3BhY2VHcm91cGVkLmdldChuYW1lc3BhY2UpIVxuXG4gICAgaWYgKGRlZi5raW5kID09PSBUeXBlRGVmS2luZC5TdHJ1Y3QpIHtcbiAgICAgIGdyb3VwLnB1c2goZGVmKVxuICAgICAgY2xhc3NEZWZzLnNldChkZWYubmFtZSwgZGVmKVxuICAgIH0gZWxzZSBpZiAoZGVmLmtpbmQgPT09IFR5cGVEZWZLaW5kLkV4dGVuZHMpIHtcbiAgICAgIGNvbnN0IGNsYXNzRGVmID0gY2xhc3NEZWZzLmdldChkZWYubmFtZSlcbiAgICAgIGlmIChjbGFzc0RlZikge1xuICAgICAgICBjbGFzc0RlZi5leHRlbmRzID0gZGVmLmRlZlxuICAgICAgfVxuICAgIH0gZWxzZSBpZiAoZGVmLmtpbmQgPT09IFR5cGVEZWZLaW5kLkltcGwpIHtcbiAgICAgIC8vIG1lcmdlIGBpbXBsYCBpbnRvIGNsYXNzIGRlZmluaXRpb25cbiAgICAgIGNvbnN0IGNsYXNzRGVmID0gY2xhc3NEZWZzLmdldChkZWYubmFtZSlcbiAgICAgIGlmIChjbGFzc0RlZikge1xuICAgICAgICBpZiAoY2xhc3NEZWYuZGVmKSB7XG4gICAgICAgICAgY2xhc3NEZWYuZGVmICs9ICdcXG4nXG4gICAgICAgIH1cblxuICAgICAgICBjbGFzc0RlZi5kZWYgKz0gZGVmLmRlZlxuICAgICAgICAvLyBDb252ZXJ0IGFueSByZW1haW5pbmcgXFxuIHNlcXVlbmNlcyBpbiB0aGUgbWVyZ2VkIGRlZiB0byBhY3R1YWwgbmV3bGluZXNcbiAgICAgICAgaWYgKGNsYXNzRGVmLmRlZikge1xuICAgICAgICAgIGNsYXNzRGVmLmRlZiA9IGNsYXNzRGVmLmRlZi5yZXBsYWNlKC9cXFxcbi9nLCAnXFxuJylcbiAgICAgICAgfVxuICAgICAgfVxuICAgIH0gZWxzZSB7XG4gICAgICBncm91cC5wdXNoKGRlZilcbiAgICB9XG4gIH1cblxuICByZXR1cm4gbmFtZXNwYWNlR3JvdXBlZFxufVxuXG5leHBvcnQgZnVuY3Rpb24gY29ycmVjdFN0cmluZ0lkZW50KHNyYzogc3RyaW5nLCBpZGVudDogbnVtYmVyKTogc3RyaW5nIHtcbiAgbGV0IGJyYWNrZXREZXB0aCA9IDBcbiAgY29uc3QgcmVzdWx0ID0gc3JjXG4gICAgLnNwbGl0KCdcXG4nKVxuICAgIC5tYXAoKGxpbmUpID0+IHtcbiAgICAgIGxpbmUgPSBsaW5lLnRyaW0oKVxuICAgICAgaWYgKGxpbmUgPT09ICcnKSB7XG4gICAgICAgIHJldHVybiAnJ1xuICAgICAgfVxuXG4gICAgICBjb25zdCBpc0luTXVsdGlsaW5lQ29tbWVudCA9IGxpbmUuc3RhcnRzV2l0aCgnKicpXG4gICAgICBjb25zdCBpc0Nsb3NpbmdCcmFja2V0ID0gbGluZS5lbmRzV2l0aCgnfScpXG4gICAgICBjb25zdCBpc09wZW5pbmdCcmFja2V0ID0gbGluZS5lbmRzV2l0aCgneycpXG4gICAgICBjb25zdCBpc1R5cGVEZWNsYXJhdGlvbiA9IGxpbmUuZW5kc1dpdGgoJz0nKVxuICAgICAgY29uc3QgaXNUeXBlVmFyaWFudCA9IGxpbmUuc3RhcnRzV2l0aCgnfCcpXG5cbiAgICAgIGxldCByaWdodEluZGVudCA9IGlkZW50XG4gICAgICBpZiAoKGlzT3BlbmluZ0JyYWNrZXQgfHwgaXNUeXBlRGVjbGFyYXRpb24pICYmICFpc0luTXVsdGlsaW5lQ29tbWVudCkge1xuICAgICAgICBicmFja2V0RGVwdGggKz0gMVxuICAgICAgICByaWdodEluZGVudCArPSAoYnJhY2tldERlcHRoIC0gMSkgKiAyXG4gICAgICB9IGVsc2Uge1xuICAgICAgICBpZiAoXG4gICAgICAgICAgaXNDbG9zaW5nQnJhY2tldCAmJlxuICAgICAgICAgIGJyYWNrZXREZXB0aCA+IDAgJiZcbiAgICAgICAgICAhaXNJbk11bHRpbGluZUNvbW1lbnQgJiZcbiAgICAgICAgICAhaXNUeXBlVmFyaWFudFxuICAgICAgICApIHtcbiAgICAgICAgICBicmFja2V0RGVwdGggLT0gMVxuICAgICAgICB9XG4gICAgICAgIHJpZ2h0SW5kZW50ICs9IGJyYWNrZXREZXB0aCAqIDJcbiAgICAgIH1cblxuICAgICAgaWYgKGlzSW5NdWx0aWxpbmVDb21tZW50KSB7XG4gICAgICAgIHJpZ2h0SW5kZW50ICs9IDFcbiAgICAgIH1cblxuICAgICAgY29uc3QgcyA9IGAkeycgJy5yZXBlYXQocmlnaHRJbmRlbnQpfSR7bGluZX1gXG5cbiAgICAgIHJldHVybiBzXG4gICAgfSlcbiAgICAuam9pbignXFxuJylcblxuICByZXR1cm4gcmVzdWx0XG59XG4iLCJpbXBvcnQgeyByZXNvbHZlIH0gZnJvbSAnbm9kZTpwYXRoJ1xuXG5pbXBvcnQgeyByZWFkTmFwaUNvbmZpZyB9IGZyb20gJy4vY29uZmlnLmpzJ1xuXG5pbnRlcmZhY2UgTWluaW1hbE5hcGlPcHRpb25zIHtcbiAgY3dkOiBzdHJpbmdcbiAgY29uZmlnUGF0aD86IHN0cmluZ1xuICBwYWNrYWdlSnNvblBhdGg/OiBzdHJpbmdcbn1cblxuZXhwb3J0IGFzeW5jIGZ1bmN0aW9uIHJlYWRDb25maWcob3B0aW9uczogTWluaW1hbE5hcGlPcHRpb25zKSB7XG4gIGNvbnN0IHJlc29sdmVQYXRoID0gKC4uLnBhdGhzOiBzdHJpbmdbXSkgPT4gcmVzb2x2ZShvcHRpb25zLmN3ZCwgLi4ucGF0aHMpXG4gIGNvbnN0IGNvbmZpZyA9IGF3YWl0IHJlYWROYXBpQ29uZmlnKFxuICAgIHJlc29sdmVQYXRoKG9wdGlvbnMucGFja2FnZUpzb25QYXRoID8/ICdwYWNrYWdlLmpzb24nKSxcbiAgICBvcHRpb25zLmNvbmZpZ1BhdGggPyByZXNvbHZlUGF0aChvcHRpb25zLmNvbmZpZ1BhdGgpIDogdW5kZWZpbmVkLFxuICApXG4gIHJldHVybiBjb25maWdcbn1cbiIsImltcG9ydCB7IGpvaW4sIHJlc29sdmUsIHBhcnNlIH0gZnJvbSAnbm9kZTpwYXRoJ1xuXG5pbXBvcnQgKiBhcyBjb2xvcnMgZnJvbSAnY29sb3JldHRlJ1xuXG5pbXBvcnQge1xuICBhcHBseURlZmF1bHRBcnRpZmFjdHNPcHRpb25zLFxuICB0eXBlIEFydGlmYWN0c09wdGlvbnMsXG59IGZyb20gJy4uL2RlZi9hcnRpZmFjdHMuanMnXG5pbXBvcnQge1xuICByZWFkTmFwaUNvbmZpZyxcbiAgZGVidWdGYWN0b3J5LFxuICByZWFkRmlsZUFzeW5jLFxuICB3cml0ZUZpbGVBc3luYyxcbiAgVW5pQXJjaHNCeVBsYXRmb3JtLFxuICByZWFkZGlyQXN5bmMsXG59IGZyb20gJy4uL3V0aWxzL2luZGV4LmpzJ1xuXG5jb25zdCBkZWJ1ZyA9IGRlYnVnRmFjdG9yeSgnYXJ0aWZhY3RzJylcblxuZXhwb3J0IGFzeW5jIGZ1bmN0aW9uIGNvbGxlY3RBcnRpZmFjdHModXNlck9wdGlvbnM6IEFydGlmYWN0c09wdGlvbnMpIHtcbiAgY29uc3Qgb3B0aW9ucyA9IGFwcGx5RGVmYXVsdEFydGlmYWN0c09wdGlvbnModXNlck9wdGlvbnMpXG5cbiAgY29uc3QgcmVzb2x2ZVBhdGggPSAoLi4ucGF0aHM6IHN0cmluZ1tdKSA9PiByZXNvbHZlKG9wdGlvbnMuY3dkLCAuLi5wYXRocylcbiAgY29uc3QgcGFja2FnZUpzb25QYXRoID0gcmVzb2x2ZVBhdGgob3B0aW9ucy5wYWNrYWdlSnNvblBhdGgpXG4gIGNvbnN0IHsgdGFyZ2V0cywgYmluYXJ5TmFtZSwgcGFja2FnZU5hbWUgfSA9IGF3YWl0IHJlYWROYXBpQ29uZmlnKFxuICAgIHBhY2thZ2VKc29uUGF0aCxcbiAgICBvcHRpb25zLmNvbmZpZ1BhdGggPyByZXNvbHZlUGF0aChvcHRpb25zLmNvbmZpZ1BhdGgpIDogdW5kZWZpbmVkLFxuICApXG5cbiAgY29uc3QgZGlzdERpcnMgPSB0YXJnZXRzLm1hcCgocGxhdGZvcm0pID0+XG4gICAgam9pbihvcHRpb25zLmN3ZCwgb3B0aW9ucy5ucG1EaXIsIHBsYXRmb3JtLnBsYXRmb3JtQXJjaEFCSSksXG4gIClcblxuICBjb25zdCB1bml2ZXJzYWxTb3VyY2VCaW5zID0gbmV3IFNldChcbiAgICB0YXJnZXRzXG4gICAgICAuZmlsdGVyKChwbGF0Zm9ybSkgPT4gcGxhdGZvcm0uYXJjaCA9PT0gJ3VuaXZlcnNhbCcpXG4gICAgICAuZmxhdE1hcCgocCkgPT5cbiAgICAgICAgVW5pQXJjaHNCeVBsYXRmb3JtW3AucGxhdGZvcm1dPy5tYXAoKGEpID0+IGAke3AucGxhdGZvcm19LSR7YX1gKSxcbiAgICAgIClcbiAgICAgIC5maWx0ZXIoQm9vbGVhbikgYXMgc3RyaW5nW10sXG4gIClcblxuICBhd2FpdCBjb2xsZWN0Tm9kZUJpbmFyaWVzKGpvaW4ob3B0aW9ucy5jd2QsIG9wdGlvbnMub3V0cHV0RGlyKSkudGhlbihcbiAgICAob3V0cHV0KSA9PlxuICAgICAgUHJvbWlzZS5hbGwoXG4gICAgICAgIG91dHB1dC5tYXAoYXN5bmMgKGZpbGVQYXRoKSA9PiB7XG4gICAgICAgICAgZGVidWcuaW5mbyhgUmVhZCBbJHtjb2xvcnMueWVsbG93QnJpZ2h0KGZpbGVQYXRoKX1dYClcbiAgICAgICAgICBjb25zdCBzb3VyY2VDb250ZW50ID0gYXdhaXQgcmVhZEZpbGVBc3luYyhmaWxlUGF0aClcbiAgICAgICAgICBjb25zdCBwYXJzZWROYW1lID0gcGFyc2UoZmlsZVBhdGgpXG4gICAgICAgICAgY29uc3QgdGVybXMgPSBwYXJzZWROYW1lLm5hbWUuc3BsaXQoJy4nKVxuICAgICAgICAgIGNvbnN0IHBsYXRmb3JtQXJjaEFCSSA9IHRlcm1zLnBvcCgpIVxuICAgICAgICAgIGNvbnN0IF9iaW5hcnlOYW1lID0gdGVybXMuam9pbignLicpXG5cbiAgICAgICAgICBpZiAoX2JpbmFyeU5hbWUgIT09IGJpbmFyeU5hbWUpIHtcbiAgICAgICAgICAgIGRlYnVnLndhcm4oXG4gICAgICAgICAgICAgIGBbJHtfYmluYXJ5TmFtZX1dIGlzIG5vdCBtYXRjaGVkIHdpdGggWyR7YmluYXJ5TmFtZX1dLCBza2lwYCxcbiAgICAgICAgICAgIClcbiAgICAgICAgICAgIHJldHVyblxuICAgICAgICAgIH1cbiAgICAgICAgICBjb25zdCBkaXIgPSBkaXN0RGlycy5maW5kKChkaXIpID0+IGRpci5pbmNsdWRlcyhwbGF0Zm9ybUFyY2hBQkkpKVxuICAgICAgICAgIGlmICghZGlyICYmIHVuaXZlcnNhbFNvdXJjZUJpbnMuaGFzKHBsYXRmb3JtQXJjaEFCSSkpIHtcbiAgICAgICAgICAgIGRlYnVnLndhcm4oXG4gICAgICAgICAgICAgIGBbJHtwbGF0Zm9ybUFyY2hBQkl9XSBoYXMgbm8gZGlzdCBkaXIgYnV0IGl0IGlzIHNvdXJjZSBiaW4gZm9yIHVuaXZlcnNhbCBhcmNoLCBza2lwYCxcbiAgICAgICAgICAgIClcbiAgICAgICAgICAgIHJldHVyblxuICAgICAgICAgIH1cbiAgICAgICAgICBpZiAoIWRpcikge1xuICAgICAgICAgICAgdGhyb3cgbmV3IEVycm9yKGBObyBkaXN0IGRpciBmb3VuZCBmb3IgJHtmaWxlUGF0aH1gKVxuICAgICAgICAgIH1cblxuICAgICAgICAgIGNvbnN0IGRpc3RGaWxlUGF0aCA9IGpvaW4oZGlyLCBwYXJzZWROYW1lLmJhc2UpXG4gICAgICAgICAgZGVidWcuaW5mbyhcbiAgICAgICAgICAgIGBXcml0ZSBmaWxlIGNvbnRlbnQgdG8gWyR7Y29sb3JzLnllbGxvd0JyaWdodChkaXN0RmlsZVBhdGgpfV1gLFxuICAgICAgICAgIClcbiAgICAgICAgICBhd2FpdCB3cml0ZUZpbGVBc3luYyhkaXN0RmlsZVBhdGgsIHNvdXJjZUNvbnRlbnQpXG4gICAgICAgICAgY29uc3QgZGlzdEZpbGVQYXRoTG9jYWwgPSBqb2luKFxuICAgICAgICAgICAgcGFyc2UocGFja2FnZUpzb25QYXRoKS5kaXIsXG4gICAgICAgICAgICBwYXJzZWROYW1lLmJhc2UsXG4gICAgICAgICAgKVxuICAgICAgICAgIGRlYnVnLmluZm8oXG4gICAgICAgICAgICBgV3JpdGUgZmlsZSBjb250ZW50IHRvIFske2NvbG9ycy55ZWxsb3dCcmlnaHQoZGlzdEZpbGVQYXRoTG9jYWwpfV1gLFxuICAgICAgICAgIClcbiAgICAgICAgICBhd2FpdCB3cml0ZUZpbGVBc3luYyhkaXN0RmlsZVBhdGhMb2NhbCwgc291cmNlQ29udGVudClcbiAgICAgICAgfSksXG4gICAgICApLFxuICApXG5cbiAgY29uc3Qgd2FzaVRhcmdldCA9IHRhcmdldHMuZmluZCgodCkgPT4gdC5wbGF0Zm9ybSA9PT0gJ3dhc2knKVxuICBpZiAod2FzaVRhcmdldCkge1xuICAgIGNvbnN0IHdhc2lEaXIgPSBqb2luKFxuICAgICAgb3B0aW9ucy5jd2QsXG4gICAgICBvcHRpb25zLm5wbURpcixcbiAgICAgIHdhc2lUYXJnZXQucGxhdGZvcm1BcmNoQUJJLFxuICAgIClcbiAgICBjb25zdCBjanNGaWxlID0gam9pbihcbiAgICAgIG9wdGlvbnMuYnVpbGRPdXRwdXREaXIgPz8gb3B0aW9ucy5jd2QsXG4gICAgICBgJHtiaW5hcnlOYW1lfS53YXNpLmNqc2AsXG4gICAgKVxuICAgIGNvbnN0IHdvcmtlckZpbGUgPSBqb2luKFxuICAgICAgb3B0aW9ucy5idWlsZE91dHB1dERpciA/PyBvcHRpb25zLmN3ZCxcbiAgICAgIGB3YXNpLXdvcmtlci5tanNgLFxuICAgIClcbiAgICBjb25zdCBicm93c2VyRW50cnkgPSBqb2luKFxuICAgICAgb3B0aW9ucy5idWlsZE91dHB1dERpciA/PyBvcHRpb25zLmN3ZCxcbiAgICAgIGAke2JpbmFyeU5hbWV9Lndhc2ktYnJvd3Nlci5qc2AsXG4gICAgKVxuICAgIGNvbnN0IGJyb3dzZXJXb3JrZXJGaWxlID0gam9pbihcbiAgICAgIG9wdGlvbnMuYnVpbGRPdXRwdXREaXIgPz8gb3B0aW9ucy5jd2QsXG4gICAgICBgd2FzaS13b3JrZXItYnJvd3Nlci5tanNgLFxuICAgIClcbiAgICBkZWJ1Zy5pbmZvKFxuICAgICAgYE1vdmUgd2FzaSBiaW5kaW5nIGZpbGUgWyR7Y29sb3JzLnllbGxvd0JyaWdodChcbiAgICAgICAgY2pzRmlsZSxcbiAgICAgICl9XSB0byBbJHtjb2xvcnMueWVsbG93QnJpZ2h0KHdhc2lEaXIpfV1gLFxuICAgIClcbiAgICBhd2FpdCB3cml0ZUZpbGVBc3luYyhcbiAgICAgIGpvaW4od2FzaURpciwgYCR7YmluYXJ5TmFtZX0ud2FzaS5janNgKSxcbiAgICAgIGF3YWl0IHJlYWRGaWxlQXN5bmMoY2pzRmlsZSksXG4gICAgKVxuICAgIGRlYnVnLmluZm8oXG4gICAgICBgTW92ZSB3YXNpIHdvcmtlciBmaWxlIFske2NvbG9ycy55ZWxsb3dCcmlnaHQoXG4gICAgICAgIHdvcmtlckZpbGUsXG4gICAgICApfV0gdG8gWyR7Y29sb3JzLnllbGxvd0JyaWdodCh3YXNpRGlyKX1dYCxcbiAgICApXG4gICAgYXdhaXQgd3JpdGVGaWxlQXN5bmMoXG4gICAgICBqb2luKHdhc2lEaXIsIGB3YXNpLXdvcmtlci5tanNgKSxcbiAgICAgIGF3YWl0IHJlYWRGaWxlQXN5bmMod29ya2VyRmlsZSksXG4gICAgKVxuICAgIGRlYnVnLmluZm8oXG4gICAgICBgTW92ZSB3YXNpIGJyb3dzZXIgZW50cnkgZmlsZSBbJHtjb2xvcnMueWVsbG93QnJpZ2h0KFxuICAgICAgICBicm93c2VyRW50cnksXG4gICAgICApfV0gdG8gWyR7Y29sb3JzLnllbGxvd0JyaWdodCh3YXNpRGlyKX1dYCxcbiAgICApXG4gICAgYXdhaXQgd3JpdGVGaWxlQXN5bmMoXG4gICAgICBqb2luKHdhc2lEaXIsIGAke2JpbmFyeU5hbWV9Lndhc2ktYnJvd3Nlci5qc2ApLFxuICAgICAgLy8gaHR0cHM6Ly9naXRodWIuY29tL3ZpdGVqcy92aXRlL2lzc3Vlcy84NDI3XG4gICAgICAoYXdhaXQgcmVhZEZpbGVBc3luYyhicm93c2VyRW50cnksICd1dGY4JykpLnJlcGxhY2UoXG4gICAgICAgIGBuZXcgVVJMKCcuL3dhc2ktd29ya2VyLWJyb3dzZXIubWpzJywgaW1wb3J0Lm1ldGEudXJsKWAsXG4gICAgICAgIGBuZXcgVVJMKCcke3BhY2thZ2VOYW1lfS13YXNtMzItd2FzaS93YXNpLXdvcmtlci1icm93c2VyLm1qcycsIGltcG9ydC5tZXRhLnVybClgLFxuICAgICAgKSxcbiAgICApXG4gICAgZGVidWcuaW5mbyhcbiAgICAgIGBNb3ZlIHdhc2kgYnJvd3NlciB3b3JrZXIgZmlsZSBbJHtjb2xvcnMueWVsbG93QnJpZ2h0KFxuICAgICAgICBicm93c2VyV29ya2VyRmlsZSxcbiAgICAgICl9XSB0byBbJHtjb2xvcnMueWVsbG93QnJpZ2h0KHdhc2lEaXIpfV1gLFxuICAgIClcbiAgICBhd2FpdCB3cml0ZUZpbGVBc3luYyhcbiAgICAgIGpvaW4od2FzaURpciwgYHdhc2ktd29ya2VyLWJyb3dzZXIubWpzYCksXG4gICAgICBhd2FpdCByZWFkRmlsZUFzeW5jKGJyb3dzZXJXb3JrZXJGaWxlKSxcbiAgICApXG4gIH1cbn1cblxuYXN5bmMgZnVuY3Rpb24gY29sbGVjdE5vZGVCaW5hcmllcyhyb290OiBzdHJpbmcpIHtcbiAgY29uc3QgZmlsZXMgPSBhd2FpdCByZWFkZGlyQXN5bmMocm9vdCwgeyB3aXRoRmlsZVR5cGVzOiB0cnVlIH0pXG4gIGNvbnN0IG5vZGVCaW5hcmllcyA9IGZpbGVzXG4gICAgLmZpbHRlcihcbiAgICAgIChmaWxlKSA9PlxuICAgICAgICBmaWxlLmlzRmlsZSgpICYmXG4gICAgICAgIChmaWxlLm5hbWUuZW5kc1dpdGgoJy5ub2RlJykgfHwgZmlsZS5uYW1lLmVuZHNXaXRoKCcud2FzbScpKSxcbiAgICApXG4gICAgLm1hcCgoZmlsZSkgPT4gam9pbihyb290LCBmaWxlLm5hbWUpKVxuXG4gIGNvbnN0IGRpcnMgPSBmaWxlcy5maWx0ZXIoKGZpbGUpID0+IGZpbGUuaXNEaXJlY3RvcnkoKSlcbiAgZm9yIChjb25zdCBkaXIgb2YgZGlycykge1xuICAgIGlmIChkaXIubmFtZSAhPT0gJ25vZGVfbW9kdWxlcycpIHtcbiAgICAgIG5vZGVCaW5hcmllcy5wdXNoKC4uLihhd2FpdCBjb2xsZWN0Tm9kZUJpbmFyaWVzKGpvaW4ocm9vdCwgZGlyLm5hbWUpKSkpXG4gICAgfVxuICB9XG4gIHJldHVybiBub2RlQmluYXJpZXNcbn1cbiIsImV4cG9ydCBmdW5jdGlvbiBjcmVhdGVDanNCaW5kaW5nKFxuICBsb2NhbE5hbWU6IHN0cmluZyxcbiAgcGtnTmFtZTogc3RyaW5nLFxuICBpZGVudHM6IHN0cmluZ1tdLFxuICBwYWNrYWdlVmVyc2lvbj86IHN0cmluZyxcbik6IHN0cmluZyB7XG4gIHJldHVybiBgJHtiaW5kaW5nSGVhZGVyfVxuJHtjcmVhdGVDb21tb25CaW5kaW5nKGxvY2FsTmFtZSwgcGtnTmFtZSwgcGFja2FnZVZlcnNpb24pfVxubW9kdWxlLmV4cG9ydHMgPSBuYXRpdmVCaW5kaW5nXG4ke2lkZW50c1xuICAubWFwKChpZGVudCkgPT4gYG1vZHVsZS5leHBvcnRzLiR7aWRlbnR9ID0gbmF0aXZlQmluZGluZy4ke2lkZW50fWApXG4gIC5qb2luKCdcXG4nKX1cbmBcbn1cblxuZXhwb3J0IGZ1bmN0aW9uIGNyZWF0ZUVzbUJpbmRpbmcoXG4gIGxvY2FsTmFtZTogc3RyaW5nLFxuICBwa2dOYW1lOiBzdHJpbmcsXG4gIGlkZW50czogc3RyaW5nW10sXG4gIHBhY2thZ2VWZXJzaW9uPzogc3RyaW5nLFxuKTogc3RyaW5nIHtcbiAgcmV0dXJuIGAke2JpbmRpbmdIZWFkZXJ9XG5pbXBvcnQgeyBjcmVhdGVSZXF1aXJlIH0gZnJvbSAnbm9kZTptb2R1bGUnXG5jb25zdCByZXF1aXJlID0gY3JlYXRlUmVxdWlyZShpbXBvcnQubWV0YS51cmwpXG5jb25zdCBfX2Rpcm5hbWUgPSBuZXcgVVJMKCcuJywgaW1wb3J0Lm1ldGEudXJsKS5wYXRobmFtZVxuXG4ke2NyZWF0ZUNvbW1vbkJpbmRpbmcobG9jYWxOYW1lLCBwa2dOYW1lLCBwYWNrYWdlVmVyc2lvbil9XG5jb25zdCB7ICR7aWRlbnRzLmpvaW4oJywgJyl9IH0gPSBuYXRpdmVCaW5kaW5nXG4ke2lkZW50cy5tYXAoKGlkZW50KSA9PiBgZXhwb3J0IHsgJHtpZGVudH0gfWApLmpvaW4oJ1xcbicpfVxuYFxufVxuXG5jb25zdCBiaW5kaW5nSGVhZGVyID0gYC8vIHByZXR0aWVyLWlnbm9yZVxuLyogZXNsaW50LWRpc2FibGUgKi9cbi8vIEB0cy1ub2NoZWNrXG4vKiBhdXRvLWdlbmVyYXRlZCBieSBOQVBJLVJTICovXG5gXG5cbmZ1bmN0aW9uIGNyZWF0ZUNvbW1vbkJpbmRpbmcoXG4gIGxvY2FsTmFtZTogc3RyaW5nLFxuICBwa2dOYW1lOiBzdHJpbmcsXG4gIHBhY2thZ2VWZXJzaW9uPzogc3RyaW5nLFxuKTogc3RyaW5nIHtcbiAgZnVuY3Rpb24gcmVxdWlyZVR1cGxlKHR1cGxlOiBzdHJpbmcsIGlkZW50U2l6ZSA9IDgpIHtcbiAgICBjb25zdCBpZGVudExvdyA9ICcgJy5yZXBlYXQoaWRlbnRTaXplIC0gMilcbiAgICBjb25zdCBpZGVudCA9ICcgJy5yZXBlYXQoaWRlbnRTaXplKVxuICAgIGNvbnN0IHZlcnNpb25DaGVjayA9IHBhY2thZ2VWZXJzaW9uXG4gICAgICA/IGBcbiR7aWRlbnRMb3d9dHJ5IHtcbiR7aWRlbnR9Y29uc3QgYmluZGluZyA9IHJlcXVpcmUoJyR7cGtnTmFtZX0tJHt0dXBsZX0nKVxuJHtpZGVudH1jb25zdCBiaW5kaW5nUGFja2FnZVZlcnNpb24gPSByZXF1aXJlKCcke3BrZ05hbWV9LSR7dHVwbGV9L3BhY2thZ2UuanNvbicpLnZlcnNpb25cbiR7aWRlbnR9aWYgKGJpbmRpbmdQYWNrYWdlVmVyc2lvbiAhPT0gJyR7cGFja2FnZVZlcnNpb259JyAmJiBwcm9jZXNzLmVudi5OQVBJX1JTX0VORk9SQ0VfVkVSU0lPTl9DSEVDSyAmJiBwcm9jZXNzLmVudi5OQVBJX1JTX0VORk9SQ0VfVkVSU0lPTl9DSEVDSyAhPT0gJzAnKSB7XG4ke2lkZW50fSAgdGhyb3cgbmV3IEVycm9yKFxcYE5hdGl2ZSBiaW5kaW5nIHBhY2thZ2UgdmVyc2lvbiBtaXNtYXRjaCwgZXhwZWN0ZWQgJHtwYWNrYWdlVmVyc2lvbn0gYnV0IGdvdCBcXCR7YmluZGluZ1BhY2thZ2VWZXJzaW9ufS4gWW91IGNhbiByZWluc3RhbGwgZGVwZW5kZW5jaWVzIHRvIGZpeCB0aGlzIGlzc3VlLlxcYClcbiR7aWRlbnR9fVxuJHtpZGVudH1yZXR1cm4gYmluZGluZ1xuJHtpZGVudExvd319IGNhdGNoIChlKSB7XG4ke2lkZW50fWxvYWRFcnJvcnMucHVzaChlKVxuJHtpZGVudExvd319YFxuICAgICAgOiBgXG4ke2lkZW50TG93fXRyeSB7XG4ke2lkZW50fXJldHVybiByZXF1aXJlKCcke3BrZ05hbWV9LSR7dHVwbGV9JylcbiR7aWRlbnRMb3d9fSBjYXRjaCAoZSkge1xuJHtpZGVudH1sb2FkRXJyb3JzLnB1c2goZSlcbiR7aWRlbnRMb3d9fWBcbiAgICByZXR1cm4gYHRyeSB7XG4ke2lkZW50fXJldHVybiByZXF1aXJlKCcuLyR7bG9jYWxOYW1lfS4ke3R1cGxlfS5ub2RlJylcbiR7aWRlbnRMb3d9fSBjYXRjaCAoZSkge1xuJHtpZGVudH1sb2FkRXJyb3JzLnB1c2goZSlcbiR7aWRlbnRMb3d9fSR7dmVyc2lvbkNoZWNrfWBcbiAgfVxuXG4gIHJldHVybiBgY29uc3QgeyByZWFkRmlsZVN5bmMgfSA9IHJlcXVpcmUoJ25vZGU6ZnMnKVxubGV0IG5hdGl2ZUJpbmRpbmcgPSBudWxsXG5jb25zdCBsb2FkRXJyb3JzID0gW11cblxuY29uc3QgaXNNdXNsID0gKCkgPT4ge1xuICBsZXQgbXVzbCA9IGZhbHNlXG4gIGlmIChwcm9jZXNzLnBsYXRmb3JtID09PSAnbGludXgnKSB7XG4gICAgbXVzbCA9IGlzTXVzbEZyb21GaWxlc3lzdGVtKClcbiAgICBpZiAobXVzbCA9PT0gbnVsbCkge1xuICAgICAgbXVzbCA9IGlzTXVzbEZyb21SZXBvcnQoKVxuICAgIH1cbiAgICBpZiAobXVzbCA9PT0gbnVsbCkge1xuICAgICAgbXVzbCA9IGlzTXVzbEZyb21DaGlsZFByb2Nlc3MoKVxuICAgIH1cbiAgfVxuICByZXR1cm4gbXVzbFxufVxuXG5jb25zdCBpc0ZpbGVNdXNsID0gKGYpID0+IGYuaW5jbHVkZXMoJ2xpYmMubXVzbC0nKSB8fCBmLmluY2x1ZGVzKCdsZC1tdXNsLScpXG5cbmNvbnN0IGlzTXVzbEZyb21GaWxlc3lzdGVtID0gKCkgPT4ge1xuICB0cnkge1xuICAgIHJldHVybiByZWFkRmlsZVN5bmMoJy91c3IvYmluL2xkZCcsICd1dGYtOCcpLmluY2x1ZGVzKCdtdXNsJylcbiAgfSBjYXRjaCB7XG4gICAgcmV0dXJuIG51bGxcbiAgfVxufVxuXG5jb25zdCBpc011c2xGcm9tUmVwb3J0ID0gKCkgPT4ge1xuICBsZXQgcmVwb3J0ID0gbnVsbFxuICBpZiAodHlwZW9mIHByb2Nlc3MucmVwb3J0Py5nZXRSZXBvcnQgPT09ICdmdW5jdGlvbicpIHtcbiAgICBwcm9jZXNzLnJlcG9ydC5leGNsdWRlTmV0d29yayA9IHRydWVcbiAgICByZXBvcnQgPSBwcm9jZXNzLnJlcG9ydC5nZXRSZXBvcnQoKVxuICB9XG4gIGlmICghcmVwb3J0KSB7XG4gICAgcmV0dXJuIG51bGxcbiAgfVxuICBpZiAocmVwb3J0LmhlYWRlciAmJiByZXBvcnQuaGVhZGVyLmdsaWJjVmVyc2lvblJ1bnRpbWUpIHtcbiAgICByZXR1cm4gZmFsc2VcbiAgfVxuICBpZiAoQXJyYXkuaXNBcnJheShyZXBvcnQuc2hhcmVkT2JqZWN0cykpIHtcbiAgICBpZiAocmVwb3J0LnNoYXJlZE9iamVjdHMuc29tZShpc0ZpbGVNdXNsKSkge1xuICAgICAgcmV0dXJuIHRydWVcbiAgICB9XG4gIH1cbiAgcmV0dXJuIGZhbHNlXG59XG5cbmNvbnN0IGlzTXVzbEZyb21DaGlsZFByb2Nlc3MgPSAoKSA9PiB7XG4gIHRyeSB7XG4gICAgcmV0dXJuIHJlcXVpcmUoJ2NoaWxkX3Byb2Nlc3MnKS5leGVjU3luYygnbGRkIC0tdmVyc2lvbicsIHsgZW5jb2Rpbmc6ICd1dGY4JyB9KS5pbmNsdWRlcygnbXVzbCcpXG4gIH0gY2F0Y2ggKGUpIHtcbiAgICAvLyBJZiB3ZSByZWFjaCB0aGlzIGNhc2UsIHdlIGRvbid0IGtub3cgaWYgdGhlIHN5c3RlbSBpcyBtdXNsIG9yIG5vdCwgc28gaXMgYmV0dGVyIHRvIGp1c3QgZmFsbGJhY2sgdG8gZmFsc2VcbiAgICByZXR1cm4gZmFsc2VcbiAgfVxufVxuXG5mdW5jdGlvbiByZXF1aXJlTmF0aXZlKCkge1xuICBpZiAocHJvY2Vzcy5lbnYuTkFQSV9SU19OQVRJVkVfTElCUkFSWV9QQVRIKSB7XG4gICAgdHJ5IHtcbiAgICAgIHJldHVybiByZXF1aXJlKHByb2Nlc3MuZW52Lk5BUElfUlNfTkFUSVZFX0xJQlJBUllfUEFUSCk7XG4gICAgfSBjYXRjaCAoZXJyKSB7XG4gICAgICBsb2FkRXJyb3JzLnB1c2goZXJyKVxuICAgIH1cbiAgfSBlbHNlIGlmIChwcm9jZXNzLnBsYXRmb3JtID09PSAnYW5kcm9pZCcpIHtcbiAgICBpZiAocHJvY2Vzcy5hcmNoID09PSAnYXJtNjQnKSB7XG4gICAgICAke3JlcXVpcmVUdXBsZSgnYW5kcm9pZC1hcm02NCcpfVxuICAgIH0gZWxzZSBpZiAocHJvY2Vzcy5hcmNoID09PSAnYXJtJykge1xuICAgICAgJHtyZXF1aXJlVHVwbGUoJ2FuZHJvaWQtYXJtLWVhYmknKX1cbiAgICB9IGVsc2Uge1xuICAgICAgbG9hZEVycm9ycy5wdXNoKG5ldyBFcnJvcihcXGBVbnN1cHBvcnRlZCBhcmNoaXRlY3R1cmUgb24gQW5kcm9pZCBcXCR7cHJvY2Vzcy5hcmNofVxcYCkpXG4gICAgfVxuICB9IGVsc2UgaWYgKHByb2Nlc3MucGxhdGZvcm0gPT09ICd3aW4zMicpIHtcbiAgICBpZiAocHJvY2Vzcy5hcmNoID09PSAneDY0Jykge1xuICAgICAgaWYgKHByb2Nlc3MuY29uZmlnPy52YXJpYWJsZXM/LnNobGliX3N1ZmZpeCA9PT0gJ2RsbC5hJyB8fCBwcm9jZXNzLmNvbmZpZz8udmFyaWFibGVzPy5ub2RlX3RhcmdldF90eXBlID09PSAnc2hhcmVkX2xpYnJhcnknKSB7XG4gICAgICAgICR7cmVxdWlyZVR1cGxlKCd3aW4zMi14NjQtZ251Jyl9XG4gICAgICB9IGVsc2Uge1xuICAgICAgICAke3JlcXVpcmVUdXBsZSgnd2luMzIteDY0LW1zdmMnKX1cbiAgICAgIH1cbiAgICB9IGVsc2UgaWYgKHByb2Nlc3MuYXJjaCA9PT0gJ2lhMzInKSB7XG4gICAgICAke3JlcXVpcmVUdXBsZSgnd2luMzItaWEzMi1tc3ZjJyl9XG4gICAgfSBlbHNlIGlmIChwcm9jZXNzLmFyY2ggPT09ICdhcm02NCcpIHtcbiAgICAgICR7cmVxdWlyZVR1cGxlKCd3aW4zMi1hcm02NC1tc3ZjJyl9XG4gICAgfSBlbHNlIHtcbiAgICAgIGxvYWRFcnJvcnMucHVzaChuZXcgRXJyb3IoXFxgVW5zdXBwb3J0ZWQgYXJjaGl0ZWN0dXJlIG9uIFdpbmRvd3M6IFxcJHtwcm9jZXNzLmFyY2h9XFxgKSlcbiAgICB9XG4gIH0gZWxzZSBpZiAocHJvY2Vzcy5wbGF0Zm9ybSA9PT0gJ2RhcndpbicpIHtcbiAgICAke3JlcXVpcmVUdXBsZSgnZGFyd2luLXVuaXZlcnNhbCcsIDYpfVxuICAgIGlmIChwcm9jZXNzLmFyY2ggPT09ICd4NjQnKSB7XG4gICAgICAke3JlcXVpcmVUdXBsZSgnZGFyd2luLXg2NCcpfVxuICAgIH0gZWxzZSBpZiAocHJvY2Vzcy5hcmNoID09PSAnYXJtNjQnKSB7XG4gICAgICAke3JlcXVpcmVUdXBsZSgnZGFyd2luLWFybTY0Jyl9XG4gICAgfSBlbHNlIHtcbiAgICAgIGxvYWRFcnJvcnMucHVzaChuZXcgRXJyb3IoXFxgVW5zdXBwb3J0ZWQgYXJjaGl0ZWN0dXJlIG9uIG1hY09TOiBcXCR7cHJvY2Vzcy5hcmNofVxcYCkpXG4gICAgfVxuICB9IGVsc2UgaWYgKHByb2Nlc3MucGxhdGZvcm0gPT09ICdmcmVlYnNkJykge1xuICAgIGlmIChwcm9jZXNzLmFyY2ggPT09ICd4NjQnKSB7XG4gICAgICAke3JlcXVpcmVUdXBsZSgnZnJlZWJzZC14NjQnKX1cbiAgICB9IGVsc2UgaWYgKHByb2Nlc3MuYXJjaCA9PT0gJ2FybTY0Jykge1xuICAgICAgJHtyZXF1aXJlVHVwbGUoJ2ZyZWVic2QtYXJtNjQnKX1cbiAgICB9IGVsc2Uge1xuICAgICAgbG9hZEVycm9ycy5wdXNoKG5ldyBFcnJvcihcXGBVbnN1cHBvcnRlZCBhcmNoaXRlY3R1cmUgb24gRnJlZUJTRDogXFwke3Byb2Nlc3MuYXJjaH1cXGApKVxuICAgIH1cbiAgfSBlbHNlIGlmIChwcm9jZXNzLnBsYXRmb3JtID09PSAnbGludXgnKSB7XG4gICAgaWYgKHByb2Nlc3MuYXJjaCA9PT0gJ3g2NCcpIHtcbiAgICAgIGlmIChpc011c2woKSkge1xuICAgICAgICAke3JlcXVpcmVUdXBsZSgnbGludXgteDY0LW11c2wnLCAxMCl9XG4gICAgICB9IGVsc2Uge1xuICAgICAgICAke3JlcXVpcmVUdXBsZSgnbGludXgteDY0LWdudScsIDEwKX1cbiAgICAgIH1cbiAgICB9IGVsc2UgaWYgKHByb2Nlc3MuYXJjaCA9PT0gJ2FybTY0Jykge1xuICAgICAgaWYgKGlzTXVzbCgpKSB7XG4gICAgICAgICR7cmVxdWlyZVR1cGxlKCdsaW51eC1hcm02NC1tdXNsJywgMTApfVxuICAgICAgfSBlbHNlIHtcbiAgICAgICAgJHtyZXF1aXJlVHVwbGUoJ2xpbnV4LWFybTY0LWdudScsIDEwKX1cbiAgICAgIH1cbiAgICB9IGVsc2UgaWYgKHByb2Nlc3MuYXJjaCA9PT0gJ2FybScpIHtcbiAgICAgIGlmIChpc011c2woKSkge1xuICAgICAgICAke3JlcXVpcmVUdXBsZSgnbGludXgtYXJtLW11c2xlYWJpaGYnLCAxMCl9XG4gICAgICB9IGVsc2Uge1xuICAgICAgICAke3JlcXVpcmVUdXBsZSgnbGludXgtYXJtLWdudWVhYmloZicsIDEwKX1cbiAgICAgIH1cbiAgICB9IGVsc2UgaWYgKHByb2Nlc3MuYXJjaCA9PT0gJ2xvb25nNjQnKSB7XG4gICAgICBpZiAoaXNNdXNsKCkpIHtcbiAgICAgICAgJHtyZXF1aXJlVHVwbGUoJ2xpbnV4LWxvb25nNjQtbXVzbCcsIDEwKX1cbiAgICAgIH0gZWxzZSB7XG4gICAgICAgICR7cmVxdWlyZVR1cGxlKCdsaW51eC1sb29uZzY0LWdudScsIDEwKX1cbiAgICAgIH1cbiAgICB9IGVsc2UgaWYgKHByb2Nlc3MuYXJjaCA9PT0gJ3Jpc2N2NjQnKSB7XG4gICAgICBpZiAoaXNNdXNsKCkpIHtcbiAgICAgICAgJHtyZXF1aXJlVHVwbGUoJ2xpbnV4LXJpc2N2NjQtbXVzbCcsIDEwKX1cbiAgICAgIH0gZWxzZSB7XG4gICAgICAgICR7cmVxdWlyZVR1cGxlKCdsaW51eC1yaXNjdjY0LWdudScsIDEwKX1cbiAgICAgIH1cbiAgICB9IGVsc2UgaWYgKHByb2Nlc3MuYXJjaCA9PT0gJ3BwYzY0Jykge1xuICAgICAgJHtyZXF1aXJlVHVwbGUoJ2xpbnV4LXBwYzY0LWdudScpfVxuICAgIH0gZWxzZSBpZiAocHJvY2Vzcy5hcmNoID09PSAnczM5MHgnKSB7XG4gICAgICAke3JlcXVpcmVUdXBsZSgnbGludXgtczM5MHgtZ251Jyl9XG4gICAgfSBlbHNlIHtcbiAgICAgIGxvYWRFcnJvcnMucHVzaChuZXcgRXJyb3IoXFxgVW5zdXBwb3J0ZWQgYXJjaGl0ZWN0dXJlIG9uIExpbnV4OiBcXCR7cHJvY2Vzcy5hcmNofVxcYCkpXG4gICAgfVxuICB9IGVsc2UgaWYgKHByb2Nlc3MucGxhdGZvcm0gPT09ICdvcGVuaGFybW9ueScpIHtcbiAgICBpZiAocHJvY2Vzcy5hcmNoID09PSAnYXJtNjQnKSB7XG4gICAgICAke3JlcXVpcmVUdXBsZSgnb3Blbmhhcm1vbnktYXJtNjQnKX1cbiAgICB9IGVsc2UgaWYgKHByb2Nlc3MuYXJjaCA9PT0gJ3g2NCcpIHtcbiAgICAgICR7cmVxdWlyZVR1cGxlKCdvcGVuaGFybW9ueS14NjQnKX1cbiAgICB9IGVsc2UgaWYgKHByb2Nlc3MuYXJjaCA9PT0gJ2FybScpIHtcbiAgICAgICR7cmVxdWlyZVR1cGxlKCdvcGVuaGFybW9ueS1hcm0nKX1cbiAgICB9IGVsc2Uge1xuICAgICAgbG9hZEVycm9ycy5wdXNoKG5ldyBFcnJvcihcXGBVbnN1cHBvcnRlZCBhcmNoaXRlY3R1cmUgb24gT3Blbkhhcm1vbnk6IFxcJHtwcm9jZXNzLmFyY2h9XFxgKSlcbiAgICB9XG4gIH0gZWxzZSB7XG4gICAgbG9hZEVycm9ycy5wdXNoKG5ldyBFcnJvcihcXGBVbnN1cHBvcnRlZCBPUzogXFwke3Byb2Nlc3MucGxhdGZvcm19LCBhcmNoaXRlY3R1cmU6IFxcJHtwcm9jZXNzLmFyY2h9XFxgKSlcbiAgfVxufVxuXG5uYXRpdmVCaW5kaW5nID0gcmVxdWlyZU5hdGl2ZSgpXG5cbmlmICghbmF0aXZlQmluZGluZyB8fCBwcm9jZXNzLmVudi5OQVBJX1JTX0ZPUkNFX1dBU0kpIHtcbiAgbGV0IHdhc2lCaW5kaW5nID0gbnVsbFxuICBsZXQgd2FzaUJpbmRpbmdFcnJvciA9IG51bGxcbiAgdHJ5IHtcbiAgICB3YXNpQmluZGluZyA9IHJlcXVpcmUoJy4vJHtsb2NhbE5hbWV9Lndhc2kuY2pzJylcbiAgICBuYXRpdmVCaW5kaW5nID0gd2FzaUJpbmRpbmdcbiAgfSBjYXRjaCAoZXJyKSB7XG4gICAgaWYgKHByb2Nlc3MuZW52Lk5BUElfUlNfRk9SQ0VfV0FTSSkge1xuICAgICAgd2FzaUJpbmRpbmdFcnJvciA9IGVyclxuICAgIH1cbiAgfVxuICBpZiAoIW5hdGl2ZUJpbmRpbmcgfHwgcHJvY2Vzcy5lbnYuTkFQSV9SU19GT1JDRV9XQVNJKSB7XG4gICAgdHJ5IHtcbiAgICAgIHdhc2lCaW5kaW5nID0gcmVxdWlyZSgnJHtwa2dOYW1lfS13YXNtMzItd2FzaScpXG4gICAgICBuYXRpdmVCaW5kaW5nID0gd2FzaUJpbmRpbmdcbiAgICB9IGNhdGNoIChlcnIpIHtcbiAgICAgIGlmIChwcm9jZXNzLmVudi5OQVBJX1JTX0ZPUkNFX1dBU0kpIHtcbiAgICAgICAgaWYgKCF3YXNpQmluZGluZ0Vycm9yKSB7XG4gICAgICAgICAgd2FzaUJpbmRpbmdFcnJvciA9IGVyclxuICAgICAgICB9IGVsc2Uge1xuICAgICAgICAgIHdhc2lCaW5kaW5nRXJyb3IuY2F1c2UgPSBlcnJcbiAgICAgICAgfVxuICAgICAgICBsb2FkRXJyb3JzLnB1c2goZXJyKVxuICAgICAgfVxuICAgIH1cbiAgfVxuICBpZiAocHJvY2Vzcy5lbnYuTkFQSV9SU19GT1JDRV9XQVNJID09PSAnZXJyb3InICYmICF3YXNpQmluZGluZykge1xuICAgIGNvbnN0IGVycm9yID0gbmV3IEVycm9yKCdXQVNJIGJpbmRpbmcgbm90IGZvdW5kIGFuZCBOQVBJX1JTX0ZPUkNFX1dBU0kgaXMgc2V0IHRvIGVycm9yJylcbiAgICBlcnJvci5jYXVzZSA9IHdhc2lCaW5kaW5nRXJyb3JcbiAgICB0aHJvdyBlcnJvclxuICB9XG59XG5cbmlmICghbmF0aXZlQmluZGluZykge1xuICBpZiAobG9hZEVycm9ycy5sZW5ndGggPiAwKSB7XG4gICAgdGhyb3cgbmV3IEVycm9yKFxuICAgICAgXFxgQ2Fubm90IGZpbmQgbmF0aXZlIGJpbmRpbmcuIFxcYCArXG4gICAgICAgIFxcYG5wbSBoYXMgYSBidWcgcmVsYXRlZCB0byBvcHRpb25hbCBkZXBlbmRlbmNpZXMgKGh0dHBzOi8vZ2l0aHViLmNvbS9ucG0vY2xpL2lzc3Vlcy80ODI4KS4gXFxgICtcbiAgICAgICAgJ1BsZWFzZSB0cnkgXFxgbnBtIGlcXGAgYWdhaW4gYWZ0ZXIgcmVtb3ZpbmcgYm90aCBwYWNrYWdlLWxvY2suanNvbiBhbmQgbm9kZV9tb2R1bGVzIGRpcmVjdG9yeS4nLFxuICAgICAge1xuICAgICAgICBjYXVzZTogbG9hZEVycm9ycy5yZWR1Y2UoKGVyciwgY3VyKSA9PiB7XG4gICAgICAgICAgY3VyLmNhdXNlID0gZXJyXG4gICAgICAgICAgcmV0dXJuIGN1clxuICAgICAgICB9KSxcbiAgICAgIH0sXG4gICAgKVxuICB9XG4gIHRocm93IG5ldyBFcnJvcihcXGBGYWlsZWQgdG8gbG9hZCBuYXRpdmUgYmluZGluZ1xcYClcbn1cbmBcbn1cbiIsImV4cG9ydCBjb25zdCBjcmVhdGVXYXNpQnJvd3NlckJpbmRpbmcgPSAoXG4gIHdhc2lGaWxlbmFtZTogc3RyaW5nLFxuICBpbml0aWFsTWVtb3J5ID0gNDAwMCxcbiAgbWF4aW11bU1lbW9yeSA9IDY1NTM2LFxuICBmcyA9IGZhbHNlLFxuICBhc3luY0luaXQgPSBmYWxzZSxcbiAgYnVmZmVyID0gZmFsc2UsXG4pID0+IHtcbiAgY29uc3QgZnNJbXBvcnQgPSBmc1xuICAgID8gYnVmZmVyXG4gICAgICA/IGBpbXBvcnQgeyBtZW1mcywgQnVmZmVyIH0gZnJvbSAnQG5hcGktcnMvd2FzbS1ydW50aW1lL2ZzJ2BcbiAgICAgIDogYGltcG9ydCB7IG1lbWZzIH0gZnJvbSAnQG5hcGktcnMvd2FzbS1ydW50aW1lL2ZzJ2BcbiAgICA6ICcnXG4gIGNvbnN0IGJ1ZmZlckltcG9ydCA9IGJ1ZmZlciAmJiAhZnMgPyBgaW1wb3J0IHsgQnVmZmVyIH0gZnJvbSAnYnVmZmVyJ2AgOiAnJ1xuICBjb25zdCB3YXNpQ3JlYXRpb24gPSBmc1xuICAgID8gYFxuZXhwb3J0IGNvbnN0IHsgZnM6IF9fZnMsIHZvbDogX192b2x1bWUgfSA9IG1lbWZzKClcblxuY29uc3QgX193YXNpID0gbmV3IF9fV0FTSSh7XG4gIHZlcnNpb246ICdwcmV2aWV3MScsXG4gIGZzOiBfX2ZzLFxuICBwcmVvcGVuczoge1xuICAgICcvJzogJy8nLFxuICB9LFxufSlgXG4gICAgOiBgXG5jb25zdCBfX3dhc2kgPSBuZXcgX19XQVNJKHtcbiAgdmVyc2lvbjogJ3ByZXZpZXcxJyxcbn0pYFxuXG4gIGNvbnN0IHdvcmtlckZzSGFuZGxlciA9IGZzXG4gICAgPyBgICAgIHdvcmtlci5hZGRFdmVudExpc3RlbmVyKCdtZXNzYWdlJywgX193YXNtQ3JlYXRlT25NZXNzYWdlRm9yRnNQcm94eShfX2ZzKSlcXG5gXG4gICAgOiAnJ1xuXG4gIGNvbnN0IGVtbmFwaUluamVjdEJ1ZmZlciA9IGJ1ZmZlclxuICAgID8gJ19fZW1uYXBpQ29udGV4dC5mZWF0dXJlLkJ1ZmZlciA9IEJ1ZmZlcidcbiAgICA6ICcnXG4gIGNvbnN0IGVtbmFwaUluc3RhbnRpYXRlSW1wb3J0ID0gYXN5bmNJbml0XG4gICAgPyBgaW5zdGFudGlhdGVOYXBpTW9kdWxlIGFzIF9fZW1uYXBpSW5zdGFudGlhdGVOYXBpTW9kdWxlYFxuICAgIDogYGluc3RhbnRpYXRlTmFwaU1vZHVsZVN5bmMgYXMgX19lbW5hcGlJbnN0YW50aWF0ZU5hcGlNb2R1bGVTeW5jYFxuICBjb25zdCBlbW5hcGlJbnN0YW50aWF0ZUNhbGwgPSBhc3luY0luaXRcbiAgICA/IGBhd2FpdCBfX2VtbmFwaUluc3RhbnRpYXRlTmFwaU1vZHVsZWBcbiAgICA6IGBfX2VtbmFwaUluc3RhbnRpYXRlTmFwaU1vZHVsZVN5bmNgXG5cbiAgcmV0dXJuIGBpbXBvcnQge1xuICBjcmVhdGVPbk1lc3NhZ2UgYXMgX193YXNtQ3JlYXRlT25NZXNzYWdlRm9yRnNQcm94eSxcbiAgZ2V0RGVmYXVsdENvbnRleHQgYXMgX19lbW5hcGlHZXREZWZhdWx0Q29udGV4dCxcbiAgJHtlbW5hcGlJbnN0YW50aWF0ZUltcG9ydH0sXG4gIFdBU0kgYXMgX19XQVNJLFxufSBmcm9tICdAbmFwaS1ycy93YXNtLXJ1bnRpbWUnXG4ke2ZzSW1wb3J0fVxuJHtidWZmZXJJbXBvcnR9XG4ke3dhc2lDcmVhdGlvbn1cblxuY29uc3QgX193YXNtVXJsID0gbmV3IFVSTCgnLi8ke3dhc2lGaWxlbmFtZX0ud2FzbScsIGltcG9ydC5tZXRhLnVybCkuaHJlZlxuY29uc3QgX19lbW5hcGlDb250ZXh0ID0gX19lbW5hcGlHZXREZWZhdWx0Q29udGV4dCgpXG4ke2VtbmFwaUluamVjdEJ1ZmZlcn1cblxuY29uc3QgX19zaGFyZWRNZW1vcnkgPSBuZXcgV2ViQXNzZW1ibHkuTWVtb3J5KHtcbiAgaW5pdGlhbDogJHtpbml0aWFsTWVtb3J5fSxcbiAgbWF4aW11bTogJHttYXhpbXVtTWVtb3J5fSxcbiAgc2hhcmVkOiB0cnVlLFxufSlcblxuY29uc3QgX193YXNtRmlsZSA9IGF3YWl0IGZldGNoKF9fd2FzbVVybCkudGhlbigocmVzKSA9PiByZXMuYXJyYXlCdWZmZXIoKSlcblxuY29uc3Qge1xuICBpbnN0YW5jZTogX19uYXBpSW5zdGFuY2UsXG4gIG1vZHVsZTogX193YXNpTW9kdWxlLFxuICBuYXBpTW9kdWxlOiBfX25hcGlNb2R1bGUsXG59ID0gJHtlbW5hcGlJbnN0YW50aWF0ZUNhbGx9KF9fd2FzbUZpbGUsIHtcbiAgY29udGV4dDogX19lbW5hcGlDb250ZXh0LFxuICBhc3luY1dvcmtQb29sU2l6ZTogNCxcbiAgd2FzaTogX193YXNpLFxuICBvbkNyZWF0ZVdvcmtlcigpIHtcbiAgICBjb25zdCB3b3JrZXIgPSBuZXcgV29ya2VyKG5ldyBVUkwoJy4vd2FzaS13b3JrZXItYnJvd3Nlci5tanMnLCBpbXBvcnQubWV0YS51cmwpLCB7XG4gICAgICB0eXBlOiAnbW9kdWxlJyxcbiAgICB9KVxuJHt3b3JrZXJGc0hhbmRsZXJ9XG4gICAgcmV0dXJuIHdvcmtlclxuICB9LFxuICBvdmVyd3JpdGVJbXBvcnRzKGltcG9ydE9iamVjdCkge1xuICAgIGltcG9ydE9iamVjdC5lbnYgPSB7XG4gICAgICAuLi5pbXBvcnRPYmplY3QuZW52LFxuICAgICAgLi4uaW1wb3J0T2JqZWN0Lm5hcGksXG4gICAgICAuLi5pbXBvcnRPYmplY3QuZW1uYXBpLFxuICAgICAgbWVtb3J5OiBfX3NoYXJlZE1lbW9yeSxcbiAgICB9XG4gICAgcmV0dXJuIGltcG9ydE9iamVjdFxuICB9LFxuICBiZWZvcmVJbml0KHsgaW5zdGFuY2UgfSkge1xuICAgIGZvciAoY29uc3QgbmFtZSBvZiBPYmplY3Qua2V5cyhpbnN0YW5jZS5leHBvcnRzKSkge1xuICAgICAgaWYgKG5hbWUuc3RhcnRzV2l0aCgnX19uYXBpX3JlZ2lzdGVyX18nKSkge1xuICAgICAgICBpbnN0YW5jZS5leHBvcnRzW25hbWVdKClcbiAgICAgIH1cbiAgICB9XG4gIH0sXG59KVxuYFxufVxuXG5leHBvcnQgY29uc3QgY3JlYXRlV2FzaUJpbmRpbmcgPSAoXG4gIHdhc21GaWxlTmFtZTogc3RyaW5nLFxuICBwYWNrYWdlTmFtZTogc3RyaW5nLFxuICBpbml0aWFsTWVtb3J5ID0gNDAwMCxcbiAgbWF4aW11bU1lbW9yeSA9IDY1NTM2LFxuKSA9PiBgLyogZXNsaW50LWRpc2FibGUgKi9cbi8qIHByZXR0aWVyLWlnbm9yZSAqL1xuXG4vKiBhdXRvLWdlbmVyYXRlZCBieSBOQVBJLVJTICovXG5cbmNvbnN0IF9fbm9kZUZzID0gcmVxdWlyZSgnbm9kZTpmcycpXG5jb25zdCBfX25vZGVQYXRoID0gcmVxdWlyZSgnbm9kZTpwYXRoJylcbmNvbnN0IHsgV0FTSTogX19ub2RlV0FTSSB9ID0gcmVxdWlyZSgnbm9kZTp3YXNpJylcbmNvbnN0IHsgV29ya2VyIH0gPSByZXF1aXJlKCdub2RlOndvcmtlcl90aHJlYWRzJylcblxuY29uc3Qge1xuICBjcmVhdGVPbk1lc3NhZ2U6IF9fd2FzbUNyZWF0ZU9uTWVzc2FnZUZvckZzUHJveHksXG4gIGdldERlZmF1bHRDb250ZXh0OiBfX2VtbmFwaUdldERlZmF1bHRDb250ZXh0LFxuICBpbnN0YW50aWF0ZU5hcGlNb2R1bGVTeW5jOiBfX2VtbmFwaUluc3RhbnRpYXRlTmFwaU1vZHVsZVN5bmMsXG59ID0gcmVxdWlyZSgnQG5hcGktcnMvd2FzbS1ydW50aW1lJylcblxuY29uc3QgX19yb290RGlyID0gX19ub2RlUGF0aC5wYXJzZShwcm9jZXNzLmN3ZCgpKS5yb290XG5cbmNvbnN0IF9fd2FzaSA9IG5ldyBfX25vZGVXQVNJKHtcbiAgdmVyc2lvbjogJ3ByZXZpZXcxJyxcbiAgZW52OiBwcm9jZXNzLmVudixcbiAgcHJlb3BlbnM6IHtcbiAgICBbX19yb290RGlyXTogX19yb290RGlyLFxuICB9XG59KVxuXG5jb25zdCBfX2VtbmFwaUNvbnRleHQgPSBfX2VtbmFwaUdldERlZmF1bHRDb250ZXh0KClcblxuY29uc3QgX19zaGFyZWRNZW1vcnkgPSBuZXcgV2ViQXNzZW1ibHkuTWVtb3J5KHtcbiAgaW5pdGlhbDogJHtpbml0aWFsTWVtb3J5fSxcbiAgbWF4aW11bTogJHttYXhpbXVtTWVtb3J5fSxcbiAgc2hhcmVkOiB0cnVlLFxufSlcblxubGV0IF9fd2FzbUZpbGVQYXRoID0gX19ub2RlUGF0aC5qb2luKF9fZGlybmFtZSwgJyR7d2FzbUZpbGVOYW1lfS53YXNtJylcbmNvbnN0IF9fd2FzbURlYnVnRmlsZVBhdGggPSBfX25vZGVQYXRoLmpvaW4oX19kaXJuYW1lLCAnJHt3YXNtRmlsZU5hbWV9LmRlYnVnLndhc20nKVxuXG5pZiAoX19ub2RlRnMuZXhpc3RzU3luYyhfX3dhc21EZWJ1Z0ZpbGVQYXRoKSkge1xuICBfX3dhc21GaWxlUGF0aCA9IF9fd2FzbURlYnVnRmlsZVBhdGhcbn0gZWxzZSBpZiAoIV9fbm9kZUZzLmV4aXN0c1N5bmMoX193YXNtRmlsZVBhdGgpKSB7XG4gIHRyeSB7XG4gICAgX193YXNtRmlsZVBhdGggPSByZXF1aXJlLnJlc29sdmUoJyR7cGFja2FnZU5hbWV9LXdhc20zMi13YXNpLyR7d2FzbUZpbGVOYW1lfS53YXNtJylcbiAgfSBjYXRjaCB7XG4gICAgdGhyb3cgbmV3IEVycm9yKCdDYW5ub3QgZmluZCAke3dhc21GaWxlTmFtZX0ud2FzbSBmaWxlLCBhbmQgJHtwYWNrYWdlTmFtZX0td2FzbTMyLXdhc2kgcGFja2FnZSBpcyBub3QgaW5zdGFsbGVkLicpXG4gIH1cbn1cblxuY29uc3QgeyBpbnN0YW5jZTogX19uYXBpSW5zdGFuY2UsIG1vZHVsZTogX193YXNpTW9kdWxlLCBuYXBpTW9kdWxlOiBfX25hcGlNb2R1bGUgfSA9IF9fZW1uYXBpSW5zdGFudGlhdGVOYXBpTW9kdWxlU3luYyhfX25vZGVGcy5yZWFkRmlsZVN5bmMoX193YXNtRmlsZVBhdGgpLCB7XG4gIGNvbnRleHQ6IF9fZW1uYXBpQ29udGV4dCxcbiAgYXN5bmNXb3JrUG9vbFNpemU6IChmdW5jdGlvbigpIHtcbiAgICBjb25zdCB0aHJlYWRzU2l6ZUZyb21FbnYgPSBOdW1iZXIocHJvY2Vzcy5lbnYuTkFQSV9SU19BU1lOQ19XT1JLX1BPT0xfU0laRSA/PyBwcm9jZXNzLmVudi5VVl9USFJFQURQT09MX1NJWkUpXG4gICAgLy8gTmFOID4gMCBpcyBmYWxzZVxuICAgIGlmICh0aHJlYWRzU2l6ZUZyb21FbnYgPiAwKSB7XG4gICAgICByZXR1cm4gdGhyZWFkc1NpemVGcm9tRW52XG4gICAgfSBlbHNlIHtcbiAgICAgIHJldHVybiA0XG4gICAgfVxuICB9KSgpLFxuICByZXVzZVdvcmtlcjogdHJ1ZSxcbiAgd2FzaTogX193YXNpLFxuICBvbkNyZWF0ZVdvcmtlcigpIHtcbiAgICBjb25zdCB3b3JrZXIgPSBuZXcgV29ya2VyKF9fbm9kZVBhdGguam9pbihfX2Rpcm5hbWUsICd3YXNpLXdvcmtlci5tanMnKSwge1xuICAgICAgZW52OiBwcm9jZXNzLmVudixcbiAgICB9KVxuICAgIHdvcmtlci5vbm1lc3NhZ2UgPSAoeyBkYXRhIH0pID0+IHtcbiAgICAgIF9fd2FzbUNyZWF0ZU9uTWVzc2FnZUZvckZzUHJveHkoX19ub2RlRnMpKGRhdGEpXG4gICAgfVxuXG4gICAgLy8gVGhlIG1haW4gdGhyZWFkIG9mIE5vZGUuanMgd2FpdHMgZm9yIGFsbCB0aGUgYWN0aXZlIGhhbmRsZXMgYmVmb3JlIGV4aXRpbmcuXG4gICAgLy8gQnV0IFJ1c3QgdGhyZWFkcyBhcmUgbmV2ZXIgd2FpdGVkIHdpdGhvdXQgXFxgdGhyZWFkOjpqb2luXFxgLlxuICAgIC8vIFNvIGhlcmUgd2UgaGFjayB0aGUgY29kZSBvZiBOb2RlLmpzIHRvIHByZXZlbnQgdGhlIHdvcmtlcnMgZnJvbSBiZWluZyByZWZlcmVuY2VkIChhY3RpdmUpLlxuICAgIC8vIEFjY29yZGluZyB0byBodHRwczovL2dpdGh1Yi5jb20vbm9kZWpzL25vZGUvYmxvYi8xOWUwZDQ3MjcyOGM3OWQ0MThiNzRiZGRmZjU4OGJlYTcwYTQwM2QwL2xpYi9pbnRlcm5hbC93b3JrZXIuanMjTDQxNSxcbiAgICAvLyBhIHdvcmtlciBpcyBjb25zaXN0IG9mIHR3byBoYW5kbGVzOiBrUHVibGljUG9ydCBhbmQga0hhbmRsZS5cbiAgICB7XG4gICAgICBjb25zdCBrUHVibGljUG9ydCA9IE9iamVjdC5nZXRPd25Qcm9wZXJ0eVN5bWJvbHMod29ya2VyKS5maW5kKHMgPT5cbiAgICAgICAgcy50b1N0cmluZygpLmluY2x1ZGVzKFwia1B1YmxpY1BvcnRcIilcbiAgICAgICk7XG4gICAgICBpZiAoa1B1YmxpY1BvcnQpIHtcbiAgICAgICAgd29ya2VyW2tQdWJsaWNQb3J0XS5yZWYgPSAoKSA9PiB7fTtcbiAgICAgIH1cblxuICAgICAgY29uc3Qga0hhbmRsZSA9IE9iamVjdC5nZXRPd25Qcm9wZXJ0eVN5bWJvbHMod29ya2VyKS5maW5kKHMgPT5cbiAgICAgICAgcy50b1N0cmluZygpLmluY2x1ZGVzKFwia0hhbmRsZVwiKVxuICAgICAgKTtcbiAgICAgIGlmIChrSGFuZGxlKSB7XG4gICAgICAgIHdvcmtlcltrSGFuZGxlXS5yZWYgPSAoKSA9PiB7fTtcbiAgICAgIH1cblxuICAgICAgd29ya2VyLnVucmVmKCk7XG4gICAgfVxuICAgIHJldHVybiB3b3JrZXJcbiAgfSxcbiAgb3ZlcndyaXRlSW1wb3J0cyhpbXBvcnRPYmplY3QpIHtcbiAgICBpbXBvcnRPYmplY3QuZW52ID0ge1xuICAgICAgLi4uaW1wb3J0T2JqZWN0LmVudixcbiAgICAgIC4uLmltcG9ydE9iamVjdC5uYXBpLFxuICAgICAgLi4uaW1wb3J0T2JqZWN0LmVtbmFwaSxcbiAgICAgIG1lbW9yeTogX19zaGFyZWRNZW1vcnksXG4gICAgfVxuICAgIHJldHVybiBpbXBvcnRPYmplY3RcbiAgfSxcbiAgYmVmb3JlSW5pdCh7IGluc3RhbmNlIH0pIHtcbiAgICBmb3IgKGNvbnN0IG5hbWUgb2YgT2JqZWN0LmtleXMoaW5zdGFuY2UuZXhwb3J0cykpIHtcbiAgICAgIGlmIChuYW1lLnN0YXJ0c1dpdGgoJ19fbmFwaV9yZWdpc3Rlcl9fJykpIHtcbiAgICAgICAgaW5zdGFuY2UuZXhwb3J0c1tuYW1lXSgpXG4gICAgICB9XG4gICAgfVxuICB9LFxufSlcbmBcbiIsImV4cG9ydCBjb25zdCBXQVNJX1dPUktFUl9URU1QTEFURSA9IGBpbXBvcnQgZnMgZnJvbSBcIm5vZGU6ZnNcIjtcbmltcG9ydCB7IGNyZWF0ZVJlcXVpcmUgfSBmcm9tIFwibm9kZTptb2R1bGVcIjtcbmltcG9ydCB7IHBhcnNlIH0gZnJvbSBcIm5vZGU6cGF0aFwiO1xuaW1wb3J0IHsgV0FTSSB9IGZyb20gXCJub2RlOndhc2lcIjtcbmltcG9ydCB7IHBhcmVudFBvcnQsIFdvcmtlciB9IGZyb20gXCJub2RlOndvcmtlcl90aHJlYWRzXCI7XG5cbmNvbnN0IHJlcXVpcmUgPSBjcmVhdGVSZXF1aXJlKGltcG9ydC5tZXRhLnVybCk7XG5cbmNvbnN0IHsgaW5zdGFudGlhdGVOYXBpTW9kdWxlU3luYywgTWVzc2FnZUhhbmRsZXIsIGdldERlZmF1bHRDb250ZXh0IH0gPSByZXF1aXJlKFwiQG5hcGktcnMvd2FzbS1ydW50aW1lXCIpO1xuXG5pZiAocGFyZW50UG9ydCkge1xuICBwYXJlbnRQb3J0Lm9uKFwibWVzc2FnZVwiLCAoZGF0YSkgPT4ge1xuICAgIGdsb2JhbFRoaXMub25tZXNzYWdlKHsgZGF0YSB9KTtcbiAgfSk7XG59XG5cbk9iamVjdC5hc3NpZ24oZ2xvYmFsVGhpcywge1xuICBzZWxmOiBnbG9iYWxUaGlzLFxuICByZXF1aXJlLFxuICBXb3JrZXIsXG4gIGltcG9ydFNjcmlwdHM6IGZ1bmN0aW9uIChmKSB7XG4gICAgOygwLCBldmFsKShmcy5yZWFkRmlsZVN5bmMoZiwgXCJ1dGY4XCIpICsgXCIvLyMgc291cmNlVVJMPVwiICsgZik7XG4gIH0sXG4gIHBvc3RNZXNzYWdlOiBmdW5jdGlvbiAobXNnKSB7XG4gICAgaWYgKHBhcmVudFBvcnQpIHtcbiAgICAgIHBhcmVudFBvcnQucG9zdE1lc3NhZ2UobXNnKTtcbiAgICB9XG4gIH0sXG59KTtcblxuY29uc3QgZW1uYXBpQ29udGV4dCA9IGdldERlZmF1bHRDb250ZXh0KCk7XG5cbmNvbnN0IF9fcm9vdERpciA9IHBhcnNlKHByb2Nlc3MuY3dkKCkpLnJvb3Q7XG5cbmNvbnN0IGhhbmRsZXIgPSBuZXcgTWVzc2FnZUhhbmRsZXIoe1xuICBvbkxvYWQoeyB3YXNtTW9kdWxlLCB3YXNtTWVtb3J5IH0pIHtcbiAgICBjb25zdCB3YXNpID0gbmV3IFdBU0koe1xuICAgICAgdmVyc2lvbjogJ3ByZXZpZXcxJyxcbiAgICAgIGVudjogcHJvY2Vzcy5lbnYsXG4gICAgICBwcmVvcGVuczoge1xuICAgICAgICBbX19yb290RGlyXTogX19yb290RGlyLFxuICAgICAgfSxcbiAgICB9KTtcblxuICAgIHJldHVybiBpbnN0YW50aWF0ZU5hcGlNb2R1bGVTeW5jKHdhc21Nb2R1bGUsIHtcbiAgICAgIGNoaWxkVGhyZWFkOiB0cnVlLFxuICAgICAgd2FzaSxcbiAgICAgIGNvbnRleHQ6IGVtbmFwaUNvbnRleHQsXG4gICAgICBvdmVyd3JpdGVJbXBvcnRzKGltcG9ydE9iamVjdCkge1xuICAgICAgICBpbXBvcnRPYmplY3QuZW52ID0ge1xuICAgICAgICAgIC4uLmltcG9ydE9iamVjdC5lbnYsXG4gICAgICAgICAgLi4uaW1wb3J0T2JqZWN0Lm5hcGksXG4gICAgICAgICAgLi4uaW1wb3J0T2JqZWN0LmVtbmFwaSxcbiAgICAgICAgICBtZW1vcnk6IHdhc21NZW1vcnlcbiAgICAgICAgfTtcbiAgICAgIH0sXG4gICAgfSk7XG4gIH0sXG59KTtcblxuZ2xvYmFsVGhpcy5vbm1lc3NhZ2UgPSBmdW5jdGlvbiAoZSkge1xuICBoYW5kbGVyLmhhbmRsZShlKTtcbn07XG5gXG5cbmV4cG9ydCBjb25zdCBjcmVhdGVXYXNpQnJvd3NlcldvcmtlckJpbmRpbmcgPSAoZnM6IGJvb2xlYW4pID0+IHtcbiAgY29uc3QgZnNJbXBvcnQgPSBmc1xuICAgID8gYGltcG9ydCB7IGluc3RhbnRpYXRlTmFwaU1vZHVsZVN5bmMsIE1lc3NhZ2VIYW5kbGVyLCBXQVNJLCBjcmVhdGVGc1Byb3h5IH0gZnJvbSAnQG5hcGktcnMvd2FzbS1ydW50aW1lJ1xuaW1wb3J0IHsgbWVtZnNFeHBvcnRlZCBhcyBfX21lbWZzRXhwb3J0ZWQgfSBmcm9tICdAbmFwaS1ycy93YXNtLXJ1bnRpbWUvZnMnXG5cbmNvbnN0IGZzID0gY3JlYXRlRnNQcm94eShfX21lbWZzRXhwb3J0ZWQpYFxuICAgIDogYGltcG9ydCB7IGluc3RhbnRpYXRlTmFwaU1vZHVsZVN5bmMsIE1lc3NhZ2VIYW5kbGVyLCBXQVNJIH0gZnJvbSAnQG5hcGktcnMvd2FzbS1ydW50aW1lJ2BcbiAgY29uc3Qgd2FzaUNyZWF0aW9uID0gZnNcbiAgICA/IGBjb25zdCB3YXNpID0gbmV3IFdBU0koe1xuICAgICAgZnMsXG4gICAgICBwcmVvcGVuczoge1xuICAgICAgICAnLyc6ICcvJyxcbiAgICAgIH0sXG4gICAgICBwcmludDogZnVuY3Rpb24gKCkge1xuICAgICAgICAvLyBlc2xpbnQtZGlzYWJsZS1uZXh0LWxpbmUgbm8tY29uc29sZVxuICAgICAgICBjb25zb2xlLmxvZy5hcHBseShjb25zb2xlLCBhcmd1bWVudHMpXG4gICAgICB9LFxuICAgICAgcHJpbnRFcnI6IGZ1bmN0aW9uKCkge1xuICAgICAgICAvLyBlc2xpbnQtZGlzYWJsZS1uZXh0LWxpbmUgbm8tY29uc29sZVxuICAgICAgICBjb25zb2xlLmVycm9yLmFwcGx5KGNvbnNvbGUsIGFyZ3VtZW50cylcbiAgICAgIH0sXG4gICAgfSlgXG4gICAgOiBgY29uc3Qgd2FzaSA9IG5ldyBXQVNJKHtcbiAgICAgIHByaW50OiBmdW5jdGlvbiAoKSB7XG4gICAgICAgIC8vIGVzbGludC1kaXNhYmxlLW5leHQtbGluZSBuby1jb25zb2xlXG4gICAgICAgIGNvbnNvbGUubG9nLmFwcGx5KGNvbnNvbGUsIGFyZ3VtZW50cylcbiAgICAgIH0sXG4gICAgICBwcmludEVycjogZnVuY3Rpb24oKSB7XG4gICAgICAgIC8vIGVzbGludC1kaXNhYmxlLW5leHQtbGluZSBuby1jb25zb2xlXG4gICAgICAgIGNvbnNvbGUuZXJyb3IuYXBwbHkoY29uc29sZSwgYXJndW1lbnRzKVxuICAgICAgfSxcbiAgICB9KWBcbiAgcmV0dXJuIGAke2ZzSW1wb3J0fVxuXG5jb25zdCBoYW5kbGVyID0gbmV3IE1lc3NhZ2VIYW5kbGVyKHtcbiAgb25Mb2FkKHsgd2FzbU1vZHVsZSwgd2FzbU1lbW9yeSB9KSB7XG4gICAgJHt3YXNpQ3JlYXRpb259XG4gICAgcmV0dXJuIGluc3RhbnRpYXRlTmFwaU1vZHVsZVN5bmMod2FzbU1vZHVsZSwge1xuICAgICAgY2hpbGRUaHJlYWQ6IHRydWUsXG4gICAgICB3YXNpLFxuICAgICAgb3ZlcndyaXRlSW1wb3J0cyhpbXBvcnRPYmplY3QpIHtcbiAgICAgICAgaW1wb3J0T2JqZWN0LmVudiA9IHtcbiAgICAgICAgICAuLi5pbXBvcnRPYmplY3QuZW52LFxuICAgICAgICAgIC4uLmltcG9ydE9iamVjdC5uYXBpLFxuICAgICAgICAgIC4uLmltcG9ydE9iamVjdC5lbW5hcGksXG4gICAgICAgICAgbWVtb3J5OiB3YXNtTWVtb3J5LFxuICAgICAgICB9XG4gICAgICB9LFxuICAgIH0pXG4gIH0sXG59KVxuXG5nbG9iYWxUaGlzLm9ubWVzc2FnZSA9IGZ1bmN0aW9uIChlKSB7XG4gIGhhbmRsZXIuaGFuZGxlKGUpXG59XG5gXG59XG4iLCJpbXBvcnQgeyBzcGF3biB9IGZyb20gJ25vZGU6Y2hpbGRfcHJvY2VzcydcbmltcG9ydCB7IGNyZWF0ZUhhc2ggfSBmcm9tICdub2RlOmNyeXB0bydcbmltcG9ydCB7IGV4aXN0c1N5bmMsIG1rZGlyU3luYywgcm1TeW5jIH0gZnJvbSAnbm9kZTpmcydcbmltcG9ydCB7IGNyZWF0ZVJlcXVpcmUgfSBmcm9tICdub2RlOm1vZHVsZSdcbmltcG9ydCB7IGhvbWVkaXIgfSBmcm9tICdub2RlOm9zJ1xuaW1wb3J0IHsgcGFyc2UsIGpvaW4sIHJlc29sdmUgfSBmcm9tICdub2RlOnBhdGgnXG5cbmltcG9ydCAqIGFzIGNvbG9ycyBmcm9tICdjb2xvcmV0dGUnXG5cbmltcG9ydCB0eXBlIHsgQnVpbGRPcHRpb25zIGFzIFJhd0J1aWxkT3B0aW9ucyB9IGZyb20gJy4uL2RlZi9idWlsZC5qcydcbmltcG9ydCB7XG4gIENMSV9WRVJTSU9OLFxuICBjb3B5RmlsZUFzeW5jLFxuICB0eXBlIENyYXRlLFxuICBkZWJ1Z0ZhY3RvcnksXG4gIERFRkFVTFRfVFlQRV9ERUZfSEVBREVSLFxuICBmaWxlRXhpc3RzLFxuICBnZXRTeXN0ZW1EZWZhdWx0VGFyZ2V0LFxuICBnZXRUYXJnZXRMaW5rZXIsXG4gIG1rZGlyQXN5bmMsXG4gIHR5cGUgTmFwaUNvbmZpZyxcbiAgcGFyc2VNZXRhZGF0YSxcbiAgcGFyc2VUcmlwbGUsXG4gIHByb2Nlc3NUeXBlRGVmLFxuICByZWFkRmlsZUFzeW5jLFxuICByZWFkTmFwaUNvbmZpZyxcbiAgdHlwZSBUYXJnZXQsXG4gIHRhcmdldFRvRW52VmFyLFxuICB0cnlJbnN0YWxsQ2FyZ29CaW5hcnksXG4gIHVubGlua0FzeW5jLFxuICB3cml0ZUZpbGVBc3luYyxcbiAgZGlyRXhpc3RzQXN5bmMsXG4gIHJlYWRkaXJBc3luYyxcbiAgdHlwZSBDYXJnb1dvcmtzcGFjZU1ldGFkYXRhLFxufSBmcm9tICcuLi91dGlscy9pbmRleC5qcydcblxuaW1wb3J0IHsgY3JlYXRlQ2pzQmluZGluZywgY3JlYXRlRXNtQmluZGluZyB9IGZyb20gJy4vdGVtcGxhdGVzL2luZGV4LmpzJ1xuaW1wb3J0IHtcbiAgY3JlYXRlV2FzaUJpbmRpbmcsXG4gIGNyZWF0ZVdhc2lCcm93c2VyQmluZGluZyxcbn0gZnJvbSAnLi90ZW1wbGF0ZXMvbG9hZC13YXNpLXRlbXBsYXRlLmpzJ1xuaW1wb3J0IHtcbiAgY3JlYXRlV2FzaUJyb3dzZXJXb3JrZXJCaW5kaW5nLFxuICBXQVNJX1dPUktFUl9URU1QTEFURSxcbn0gZnJvbSAnLi90ZW1wbGF0ZXMvd2FzaS13b3JrZXItdGVtcGxhdGUuanMnXG5cbmNvbnN0IGRlYnVnID0gZGVidWdGYWN0b3J5KCdidWlsZCcpXG5jb25zdCByZXF1aXJlID0gY3JlYXRlUmVxdWlyZShpbXBvcnQubWV0YS51cmwpXG5cbnR5cGUgT3V0cHV0S2luZCA9ICdqcycgfCAnZHRzJyB8ICdub2RlJyB8ICdleGUnIHwgJ3dhc20nXG50eXBlIE91dHB1dCA9IHsga2luZDogT3V0cHV0S2luZDsgcGF0aDogc3RyaW5nIH1cblxudHlwZSBCdWlsZE9wdGlvbnMgPSBSYXdCdWlsZE9wdGlvbnMgJiB7IGNhcmdvT3B0aW9ucz86IHN0cmluZ1tdIH1cbnR5cGUgUGFyc2VkQnVpbGRPcHRpb25zID0gT21pdDxCdWlsZE9wdGlvbnMsICdjd2QnPiAmIHsgY3dkOiBzdHJpbmcgfVxuXG5leHBvcnQgYXN5bmMgZnVuY3Rpb24gYnVpbGRQcm9qZWN0KHJhd09wdGlvbnM6IEJ1aWxkT3B0aW9ucykge1xuICBkZWJ1ZygnbmFwaSBidWlsZCBjb21tYW5kIHJlY2VpdmUgb3B0aW9uczogJU8nLCByYXdPcHRpb25zKVxuXG4gIGNvbnN0IG9wdGlvbnM6IFBhcnNlZEJ1aWxkT3B0aW9ucyA9IHtcbiAgICBkdHNDYWNoZTogdHJ1ZSxcbiAgICAuLi5yYXdPcHRpb25zLFxuICAgIGN3ZDogcmF3T3B0aW9ucy5jd2QgPz8gcHJvY2Vzcy5jd2QoKSxcbiAgfVxuXG4gIGNvbnN0IHJlc29sdmVQYXRoID0gKC4uLnBhdGhzOiBzdHJpbmdbXSkgPT4gcmVzb2x2ZShvcHRpb25zLmN3ZCwgLi4ucGF0aHMpXG5cbiAgY29uc3QgbWFuaWZlc3RQYXRoID0gcmVzb2x2ZVBhdGgob3B0aW9ucy5tYW5pZmVzdFBhdGggPz8gJ0NhcmdvLnRvbWwnKVxuICBjb25zdCBtZXRhZGF0YSA9IGF3YWl0IHBhcnNlTWV0YWRhdGEobWFuaWZlc3RQYXRoKVxuXG4gIGNvbnN0IGNyYXRlID0gbWV0YWRhdGEucGFja2FnZXMuZmluZCgocCkgPT4ge1xuICAgIC8vIHBhY2thZ2Ugd2l0aCBnaXZlbiBuYW1lXG4gICAgaWYgKG9wdGlvbnMucGFja2FnZSkge1xuICAgICAgcmV0dXJuIHAubmFtZSA9PT0gb3B0aW9ucy5wYWNrYWdlXG4gICAgfSBlbHNlIHtcbiAgICAgIHJldHVybiBwLm1hbmlmZXN0X3BhdGggPT09IG1hbmlmZXN0UGF0aFxuICAgIH1cbiAgfSlcblxuICBpZiAoIWNyYXRlKSB7XG4gICAgdGhyb3cgbmV3IEVycm9yKFxuICAgICAgJ1VuYWJsZSB0byBmaW5kIGNyYXRlIHRvIGJ1aWxkLiBJdCBzZWVtcyB5b3UgYXJlIHRyeWluZyB0byBidWlsZCBhIGNyYXRlIGluIGEgd29ya3NwYWNlLCB0cnkgdXNpbmcgYC0tcGFja2FnZWAgb3B0aW9uIHRvIHNwZWNpZnkgdGhlIHBhY2thZ2UgdG8gYnVpbGQuJyxcbiAgICApXG4gIH1cbiAgY29uc3QgY29uZmlnID0gYXdhaXQgcmVhZE5hcGlDb25maWcoXG4gICAgcmVzb2x2ZVBhdGgob3B0aW9ucy5wYWNrYWdlSnNvblBhdGggPz8gJ3BhY2thZ2UuanNvbicpLFxuICAgIG9wdGlvbnMuY29uZmlnUGF0aCA/IHJlc29sdmVQYXRoKG9wdGlvbnMuY29uZmlnUGF0aCkgOiB1bmRlZmluZWQsXG4gIClcblxuICBjb25zdCBidWlsZGVyID0gbmV3IEJ1aWxkZXIobWV0YWRhdGEsIGNyYXRlLCBjb25maWcsIG9wdGlvbnMpXG5cbiAgcmV0dXJuIGJ1aWxkZXIuYnVpbGQoKVxufVxuXG5jbGFzcyBCdWlsZGVyIHtcbiAgcHJpdmF0ZSByZWFkb25seSBhcmdzOiBzdHJpbmdbXSA9IFtdXG4gIHByaXZhdGUgcmVhZG9ubHkgZW52czogUmVjb3JkPHN0cmluZywgc3RyaW5nPiA9IHt9XG4gIHByaXZhdGUgcmVhZG9ubHkgb3V0cHV0czogT3V0cHV0W10gPSBbXVxuXG4gIHByaXZhdGUgcmVhZG9ubHkgdGFyZ2V0OiBUYXJnZXRcbiAgcHJpdmF0ZSByZWFkb25seSBjcmF0ZURpcjogc3RyaW5nXG4gIHByaXZhdGUgcmVhZG9ubHkgb3V0cHV0RGlyOiBzdHJpbmdcbiAgcHJpdmF0ZSByZWFkb25seSB0YXJnZXREaXI6IHN0cmluZ1xuICBwcml2YXRlIHJlYWRvbmx5IGVuYWJsZVR5cGVEZWY6IGJvb2xlYW4gPSBmYWxzZVxuXG4gIGNvbnN0cnVjdG9yKFxuICAgIHByaXZhdGUgcmVhZG9ubHkgbWV0YWRhdGE6IENhcmdvV29ya3NwYWNlTWV0YWRhdGEsXG4gICAgcHJpdmF0ZSByZWFkb25seSBjcmF0ZTogQ3JhdGUsXG4gICAgcHJpdmF0ZSByZWFkb25seSBjb25maWc6IE5hcGlDb25maWcsXG4gICAgcHJpdmF0ZSByZWFkb25seSBvcHRpb25zOiBQYXJzZWRCdWlsZE9wdGlvbnMsXG4gICkge1xuICAgIHRoaXMudGFyZ2V0ID0gb3B0aW9ucy50YXJnZXRcbiAgICAgID8gcGFyc2VUcmlwbGUob3B0aW9ucy50YXJnZXQpXG4gICAgICA6IHByb2Nlc3MuZW52LkNBUkdPX0JVSUxEX1RBUkdFVFxuICAgICAgICA/IHBhcnNlVHJpcGxlKHByb2Nlc3MuZW52LkNBUkdPX0JVSUxEX1RBUkdFVClcbiAgICAgICAgOiBnZXRTeXN0ZW1EZWZhdWx0VGFyZ2V0KClcbiAgICB0aGlzLmNyYXRlRGlyID0gcGFyc2UoY3JhdGUubWFuaWZlc3RfcGF0aCkuZGlyXG4gICAgdGhpcy5vdXRwdXREaXIgPSByZXNvbHZlKFxuICAgICAgdGhpcy5vcHRpb25zLmN3ZCxcbiAgICAgIG9wdGlvbnMub3V0cHV0RGlyID8/IHRoaXMuY3JhdGVEaXIsXG4gICAgKVxuICAgIHRoaXMudGFyZ2V0RGlyID1cbiAgICAgIG9wdGlvbnMudGFyZ2V0RGlyID8/XG4gICAgICBwcm9jZXNzLmVudi5DQVJHT19CVUlMRF9UQVJHRVRfRElSID8/XG4gICAgICBtZXRhZGF0YS50YXJnZXRfZGlyZWN0b3J5XG4gICAgdGhpcy5lbmFibGVUeXBlRGVmID0gdGhpcy5jcmF0ZS5kZXBlbmRlbmNpZXMuc29tZShcbiAgICAgIChkZXApID0+XG4gICAgICAgIGRlcC5uYW1lID09PSAnbmFwaS1kZXJpdmUnICYmXG4gICAgICAgIChkZXAudXNlc19kZWZhdWx0X2ZlYXR1cmVzIHx8IGRlcC5mZWF0dXJlcy5pbmNsdWRlcygndHlwZS1kZWYnKSksXG4gICAgKVxuXG4gICAgaWYgKCF0aGlzLmVuYWJsZVR5cGVEZWYpIHtcbiAgICAgIGNvbnN0IHJlcXVpcmVtZW50V2FybmluZyA9XG4gICAgICAgICdgbmFwaS1kZXJpdmVgIGNyYXRlIGlzIG5vdCB1c2VkIG9yIGB0eXBlLWRlZmAgZmVhdHVyZSBpcyBub3QgZW5hYmxlZCBmb3IgYG5hcGktZGVyaXZlYCBjcmF0ZSdcbiAgICAgIGRlYnVnLndhcm4oXG4gICAgICAgIGAke3JlcXVpcmVtZW50V2FybmluZ30uIFdpbGwgc2tpcCBiaW5kaW5nIGdlbmVyYXRpb24gZm9yIFxcYC5ub2RlXFxgLCBcXGAud2FzaVxcYCBhbmQgXFxgLmQudHNcXGAgZmlsZXMuYCxcbiAgICAgIClcblxuICAgICAgaWYgKFxuICAgICAgICB0aGlzLm9wdGlvbnMuZHRzIHx8XG4gICAgICAgIHRoaXMub3B0aW9ucy5kdHNIZWFkZXIgfHxcbiAgICAgICAgdGhpcy5jb25maWcuZHRzSGVhZGVyIHx8XG4gICAgICAgIHRoaXMuY29uZmlnLmR0c0hlYWRlckZpbGVcbiAgICAgICkge1xuICAgICAgICBkZWJ1Zy53YXJuKFxuICAgICAgICAgIGAke3JlcXVpcmVtZW50V2FybmluZ30uIFxcYGR0c1xcYCByZWxhdGVkIG9wdGlvbnMgYXJlIGVuYWJsZWQgYnV0IHdpbGwgYmUgaWdub3JlZC5gLFxuICAgICAgICApXG4gICAgICB9XG4gICAgfVxuICB9XG5cbiAgZ2V0IGNkeUxpYk5hbWUoKSB7XG4gICAgcmV0dXJuIHRoaXMuY3JhdGUudGFyZ2V0cy5maW5kKCh0KSA9PiB0LmNyYXRlX3R5cGVzLmluY2x1ZGVzKCdjZHlsaWInKSlcbiAgICAgID8ubmFtZVxuICB9XG5cbiAgZ2V0IGJpbk5hbWUoKSB7XG4gICAgcmV0dXJuIChcbiAgICAgIHRoaXMub3B0aW9ucy5iaW4gPz9cbiAgICAgIC8vIG9ubHkgYXZhaWxhYmxlIGlmIG5vdCBjZHlsaWIgb3IgYmluIG5hbWUgc3BlY2lmaWVkXG4gICAgICAodGhpcy5jZHlMaWJOYW1lXG4gICAgICAgID8gbnVsbFxuICAgICAgICA6IHRoaXMuY3JhdGUudGFyZ2V0cy5maW5kKCh0KSA9PiB0LmNyYXRlX3R5cGVzLmluY2x1ZGVzKCdiaW4nKSk/Lm5hbWUpXG4gICAgKVxuICB9XG5cbiAgYnVpbGQoKSB7XG4gICAgaWYgKCF0aGlzLmNkeUxpYk5hbWUpIHtcbiAgICAgIGNvbnN0IHdhcm5pbmcgPVxuICAgICAgICAnTWlzc2luZyBgY3JhdGUtdHlwZSA9IFtcImNkeWxpYlwiXWAgaW4gW2xpYl0gY29uZmlnLiBUaGUgYnVpbGQgcmVzdWx0IHdpbGwgbm90IGJlIGF2YWlsYWJsZSBhcyBub2RlIGFkZG9uLidcblxuICAgICAgaWYgKHRoaXMuYmluTmFtZSkge1xuICAgICAgICBkZWJ1Zy53YXJuKHdhcm5pbmcpXG4gICAgICB9IGVsc2Uge1xuICAgICAgICB0aHJvdyBuZXcgRXJyb3Iod2FybmluZylcbiAgICAgIH1cbiAgICB9XG5cbiAgICByZXR1cm4gdGhpcy5waWNrQmluYXJ5KClcbiAgICAgIC5zZXRQYWNrYWdlKClcbiAgICAgIC5zZXRGZWF0dXJlcygpXG4gICAgICAuc2V0VGFyZ2V0KClcbiAgICAgIC5waWNrQ3Jvc3NUb29sY2hhaW4oKVxuICAgICAgLnNldEVudnMoKVxuICAgICAgLnNldEJ5cGFzc0FyZ3MoKVxuICAgICAgLmV4ZWMoKVxuICB9XG5cbiAgcHJpdmF0ZSBwaWNrQ3Jvc3NUb29sY2hhaW4oKSB7XG4gICAgaWYgKCF0aGlzLm9wdGlvbnMudXNlTmFwaUNyb3NzKSB7XG4gICAgICByZXR1cm4gdGhpc1xuICAgIH1cbiAgICBpZiAodGhpcy5vcHRpb25zLnVzZUNyb3NzKSB7XG4gICAgICBkZWJ1Zy53YXJuKFxuICAgICAgICAnWW91IGFyZSB0cnlpbmcgdG8gdXNlIGJvdGggYC0tY3Jvc3NgIGFuZCBgLS11c2UtbmFwaS1jcm9zc2Agb3B0aW9ucywgYC0tdXNlLWNyb3NzYCB3aWxsIGJlIGlnbm9yZWQuJyxcbiAgICAgIClcbiAgICB9XG5cbiAgICBpZiAodGhpcy5vcHRpb25zLmNyb3NzQ29tcGlsZSkge1xuICAgICAgZGVidWcud2FybihcbiAgICAgICAgJ1lvdSBhcmUgdHJ5aW5nIHRvIHVzZSBib3RoIGAtLWNyb3NzLWNvbXBpbGVgIGFuZCBgLS11c2UtbmFwaS1jcm9zc2Agb3B0aW9ucywgYC0tY3Jvc3MtY29tcGlsZWAgd2lsbCBiZSBpZ25vcmVkLicsXG4gICAgICApXG4gICAgfVxuXG4gICAgdHJ5IHtcbiAgICAgIGNvbnN0IHsgdmVyc2lvbiwgZG93bmxvYWQgfSA9IHJlcXVpcmUoJ0BuYXBpLXJzL2Nyb3NzLXRvb2xjaGFpbicpXG5cbiAgICAgIGNvbnN0IGFsaWFzOiBSZWNvcmQ8c3RyaW5nLCBzdHJpbmc+ID0ge1xuICAgICAgICAnczM5MHgtdW5rbm93bi1saW51eC1nbnUnOiAnczM5MHgtaWJtLWxpbnV4LWdudScsXG4gICAgICB9XG5cbiAgICAgIGNvbnN0IHRvb2xjaGFpblBhdGggPSBqb2luKFxuICAgICAgICBob21lZGlyKCksXG4gICAgICAgICcubmFwaS1ycycsXG4gICAgICAgICdjcm9zcy10b29sY2hhaW4nLFxuICAgICAgICB2ZXJzaW9uLFxuICAgICAgICB0aGlzLnRhcmdldC50cmlwbGUsXG4gICAgICApXG4gICAgICBta2RpclN5bmModG9vbGNoYWluUGF0aCwgeyByZWN1cnNpdmU6IHRydWUgfSlcbiAgICAgIGlmIChleGlzdHNTeW5jKGpvaW4odG9vbGNoYWluUGF0aCwgJ3BhY2thZ2UuanNvbicpKSkge1xuICAgICAgICBkZWJ1ZyhgVG9vbGNoYWluICR7dG9vbGNoYWluUGF0aH0gZXhpc3RzLCBza2lwIGV4dHJhY3RpbmdgKVxuICAgICAgfSBlbHNlIHtcbiAgICAgICAgY29uc3QgdGFyQXJjaGl2ZSA9IGRvd25sb2FkKHByb2Nlc3MuYXJjaCwgdGhpcy50YXJnZXQudHJpcGxlKVxuICAgICAgICB0YXJBcmNoaXZlLnVucGFjayh0b29sY2hhaW5QYXRoKVxuICAgICAgfVxuICAgICAgY29uc3QgdXBwZXJDYXNlVGFyZ2V0ID0gdGFyZ2V0VG9FbnZWYXIodGhpcy50YXJnZXQudHJpcGxlKVxuICAgICAgY29uc3QgY3Jvc3NUYXJnZXROYW1lID0gYWxpYXNbdGhpcy50YXJnZXQudHJpcGxlXSA/PyB0aGlzLnRhcmdldC50cmlwbGVcbiAgICAgIGNvbnN0IGxpbmtlckVudiA9IGBDQVJHT19UQVJHRVRfJHt1cHBlckNhc2VUYXJnZXR9X0xJTktFUmBcbiAgICAgIHRoaXMuc2V0RW52SWZOb3RFeGlzdHMoXG4gICAgICAgIGxpbmtlckVudixcbiAgICAgICAgam9pbih0b29sY2hhaW5QYXRoLCAnYmluJywgYCR7Y3Jvc3NUYXJnZXROYW1lfS1nY2NgKSxcbiAgICAgIClcbiAgICAgIHRoaXMuc2V0RW52SWZOb3RFeGlzdHMoXG4gICAgICAgICdUQVJHRVRfU1lTUk9PVCcsXG4gICAgICAgIGpvaW4odG9vbGNoYWluUGF0aCwgY3Jvc3NUYXJnZXROYW1lLCAnc3lzcm9vdCcpLFxuICAgICAgKVxuICAgICAgdGhpcy5zZXRFbnZJZk5vdEV4aXN0cyhcbiAgICAgICAgJ1RBUkdFVF9BUicsXG4gICAgICAgIGpvaW4odG9vbGNoYWluUGF0aCwgJ2JpbicsIGAke2Nyb3NzVGFyZ2V0TmFtZX0tYXJgKSxcbiAgICAgIClcbiAgICAgIHRoaXMuc2V0RW52SWZOb3RFeGlzdHMoXG4gICAgICAgICdUQVJHRVRfUkFOTElCJyxcbiAgICAgICAgam9pbih0b29sY2hhaW5QYXRoLCAnYmluJywgYCR7Y3Jvc3NUYXJnZXROYW1lfS1yYW5saWJgKSxcbiAgICAgIClcbiAgICAgIHRoaXMuc2V0RW52SWZOb3RFeGlzdHMoXG4gICAgICAgICdUQVJHRVRfUkVBREVMRicsXG4gICAgICAgIGpvaW4odG9vbGNoYWluUGF0aCwgJ2JpbicsIGAke2Nyb3NzVGFyZ2V0TmFtZX0tcmVhZGVsZmApLFxuICAgICAgKVxuICAgICAgdGhpcy5zZXRFbnZJZk5vdEV4aXN0cyhcbiAgICAgICAgJ1RBUkdFVF9DX0lOQ0xVREVfUEFUSCcsXG4gICAgICAgIGpvaW4odG9vbGNoYWluUGF0aCwgY3Jvc3NUYXJnZXROYW1lLCAnc3lzcm9vdCcsICd1c3InLCAnaW5jbHVkZS8nKSxcbiAgICAgIClcbiAgICAgIHRoaXMuc2V0RW52SWZOb3RFeGlzdHMoXG4gICAgICAgICdUQVJHRVRfQ0MnLFxuICAgICAgICBqb2luKHRvb2xjaGFpblBhdGgsICdiaW4nLCBgJHtjcm9zc1RhcmdldE5hbWV9LWdjY2ApLFxuICAgICAgKVxuICAgICAgdGhpcy5zZXRFbnZJZk5vdEV4aXN0cyhcbiAgICAgICAgJ1RBUkdFVF9DWFgnLFxuICAgICAgICBqb2luKHRvb2xjaGFpblBhdGgsICdiaW4nLCBgJHtjcm9zc1RhcmdldE5hbWV9LWcrK2ApLFxuICAgICAgKVxuICAgICAgdGhpcy5zZXRFbnZJZk5vdEV4aXN0cyhcbiAgICAgICAgJ0JJTkRHRU5fRVhUUkFfQ0xBTkdfQVJHUycsXG4gICAgICAgIGAtLXN5c3Jvb3Q9JHt0aGlzLmVudnMuVEFSR0VUX1NZU1JPT1R9fWAsXG4gICAgICApXG5cbiAgICAgIGlmIChcbiAgICAgICAgcHJvY2Vzcy5lbnYuVEFSR0VUX0NDPy5zdGFydHNXaXRoKCdjbGFuZycpIHx8XG4gICAgICAgIChwcm9jZXNzLmVudi5DQz8uc3RhcnRzV2l0aCgnY2xhbmcnKSAmJiAhcHJvY2Vzcy5lbnYuVEFSR0VUX0NDKVxuICAgICAgKSB7XG4gICAgICAgIGNvbnN0IFRBUkdFVF9DRkxBR1MgPSBwcm9jZXNzLmVudi5UQVJHRVRfQ0ZMQUdTID8/ICcnXG4gICAgICAgIHRoaXMuZW52cy5UQVJHRVRfQ0ZMQUdTID0gYC0tc3lzcm9vdD0ke3RoaXMuZW52cy5UQVJHRVRfU1lTUk9PVH0gLS1nY2MtdG9vbGNoYWluPSR7dG9vbGNoYWluUGF0aH0gJHtUQVJHRVRfQ0ZMQUdTfWBcbiAgICAgIH1cbiAgICAgIGlmIChcbiAgICAgICAgKHByb2Nlc3MuZW52LkNYWD8uc3RhcnRzV2l0aCgnY2xhbmcrKycpICYmICFwcm9jZXNzLmVudi5UQVJHRVRfQ1hYKSB8fFxuICAgICAgICBwcm9jZXNzLmVudi5UQVJHRVRfQ1hYPy5zdGFydHNXaXRoKCdjbGFuZysrJylcbiAgICAgICkge1xuICAgICAgICBjb25zdCBUQVJHRVRfQ1hYRkxBR1MgPSBwcm9jZXNzLmVudi5UQVJHRVRfQ1hYRkxBR1MgPz8gJydcbiAgICAgICAgdGhpcy5lbnZzLlRBUkdFVF9DWFhGTEFHUyA9IGAtLXN5c3Jvb3Q9JHt0aGlzLmVudnMuVEFSR0VUX1NZU1JPT1R9IC0tZ2NjLXRvb2xjaGFpbj0ke3Rvb2xjaGFpblBhdGh9ICR7VEFSR0VUX0NYWEZMQUdTfWBcbiAgICAgIH1cbiAgICAgIHRoaXMuZW52cy5QQVRIID0gdGhpcy5lbnZzLlBBVEhcbiAgICAgICAgPyBgJHt0b29sY2hhaW5QYXRofS9iaW46JHt0aGlzLmVudnMuUEFUSH06JHtwcm9jZXNzLmVudi5QQVRIfWBcbiAgICAgICAgOiBgJHt0b29sY2hhaW5QYXRofS9iaW46JHtwcm9jZXNzLmVudi5QQVRIfWBcbiAgICB9IGNhdGNoIChlKSB7XG4gICAgICBkZWJ1Zy53YXJuKCdQaWNrIGNyb3NzIHRvb2xjaGFpbiBmYWlsZWQnLCBlIGFzIEVycm9yKVxuICAgICAgLy8gaWdub3JlLCBkbyBub3RoaW5nXG4gICAgfVxuICAgIHJldHVybiB0aGlzXG4gIH1cblxuICBwcml2YXRlIGV4ZWMoKSB7XG4gICAgZGVidWcoYFN0YXJ0IGJ1aWxkaW5nIGNyYXRlOiAke3RoaXMuY3JhdGUubmFtZX1gKVxuICAgIGRlYnVnKCcgICVpJywgYGNhcmdvICR7dGhpcy5hcmdzLmpvaW4oJyAnKX1gKVxuXG4gICAgY29uc3QgY29udHJvbGxlciA9IG5ldyBBYm9ydENvbnRyb2xsZXIoKVxuXG4gICAgY29uc3Qgd2F0Y2ggPSB0aGlzLm9wdGlvbnMud2F0Y2hcbiAgICBjb25zdCBidWlsZFRhc2sgPSBuZXcgUHJvbWlzZTx2b2lkPigocmVzb2x2ZSwgcmVqZWN0KSA9PiB7XG4gICAgICBpZiAodGhpcy5vcHRpb25zLnVzZUNyb3NzICYmIHRoaXMub3B0aW9ucy5jcm9zc0NvbXBpbGUpIHtcbiAgICAgICAgdGhyb3cgbmV3IEVycm9yKFxuICAgICAgICAgICdgLS11c2UtY3Jvc3NgIGFuZCBgLS1jcm9zcy1jb21waWxlYCBjYW4gbm90IGJlIHVzZWQgdG9nZXRoZXInLFxuICAgICAgICApXG4gICAgICB9XG4gICAgICBjb25zdCBjb21tYW5kID1cbiAgICAgICAgcHJvY2Vzcy5lbnYuQ0FSR08gPz8gKHRoaXMub3B0aW9ucy51c2VDcm9zcyA/ICdjcm9zcycgOiAnY2FyZ28nKVxuICAgICAgY29uc3QgYnVpbGRQcm9jZXNzID0gc3Bhd24oY29tbWFuZCwgdGhpcy5hcmdzLCB7XG4gICAgICAgIGVudjogeyAuLi5wcm9jZXNzLmVudiwgLi4udGhpcy5lbnZzIH0sXG4gICAgICAgIHN0ZGlvOiB3YXRjaCA/IFsnaW5oZXJpdCcsICdpbmhlcml0JywgJ3BpcGUnXSA6ICdpbmhlcml0JyxcbiAgICAgICAgY3dkOiB0aGlzLm9wdGlvbnMuY3dkLFxuICAgICAgICBzaWduYWw6IGNvbnRyb2xsZXIuc2lnbmFsLFxuICAgICAgfSlcblxuICAgICAgYnVpbGRQcm9jZXNzLm9uY2UoJ2V4aXQnLCAoY29kZSkgPT4ge1xuICAgICAgICBpZiAoY29kZSA9PT0gMCkge1xuICAgICAgICAgIGRlYnVnKCclaScsIGBCdWlsZCBjcmF0ZSAke3RoaXMuY3JhdGUubmFtZX0gc3VjY2Vzc2Z1bGx5IWApXG4gICAgICAgICAgcmVzb2x2ZSgpXG4gICAgICAgIH0gZWxzZSB7XG4gICAgICAgICAgcmVqZWN0KG5ldyBFcnJvcihgQnVpbGQgZmFpbGVkIHdpdGggZXhpdCBjb2RlICR7Y29kZX1gKSlcbiAgICAgICAgfVxuICAgICAgfSlcblxuICAgICAgYnVpbGRQcm9jZXNzLm9uY2UoJ2Vycm9yJywgKGUpID0+IHtcbiAgICAgICAgcmVqZWN0KG5ldyBFcnJvcihgQnVpbGQgZmFpbGVkIHdpdGggZXJyb3I6ICR7ZS5tZXNzYWdlfWAsIHsgY2F1c2U6IGUgfSkpXG4gICAgICB9KVxuXG4gICAgICAvLyB3YXRjaCBtb2RlIG9ubHksIHRoZXkgYXJlIHBpcGVkIHRocm91Z2ggc3RkZXJyXG4gICAgICBidWlsZFByb2Nlc3Muc3RkZXJyPy5vbignZGF0YScsIChkYXRhKSA9PiB7XG4gICAgICAgIGNvbnN0IG91dHB1dCA9IGRhdGEudG9TdHJpbmcoKVxuICAgICAgICBjb25zb2xlLmVycm9yKG91dHB1dClcbiAgICAgICAgaWYgKC9GaW5pc2hlZFxccyhgZGV2YHxgcmVsZWFzZWApLy50ZXN0KG91dHB1dCkpIHtcbiAgICAgICAgICB0aGlzLnBvc3RCdWlsZCgpLmNhdGNoKCgpID0+IHt9KVxuICAgICAgICB9XG4gICAgICB9KVxuICAgIH0pXG5cbiAgICByZXR1cm4ge1xuICAgICAgdGFzazogYnVpbGRUYXNrLnRoZW4oKCkgPT4gdGhpcy5wb3N0QnVpbGQoKSksXG4gICAgICBhYm9ydDogKCkgPT4gY29udHJvbGxlci5hYm9ydCgpLFxuICAgIH1cbiAgfVxuXG4gIHByaXZhdGUgcGlja0JpbmFyeSgpIHtcbiAgICBsZXQgc2V0ID0gZmFsc2VcbiAgICBpZiAodGhpcy5vcHRpb25zLndhdGNoKSB7XG4gICAgICBpZiAocHJvY2Vzcy5lbnYuQ0kpIHtcbiAgICAgICAgZGVidWcud2FybignV2F0Y2ggbW9kZSBpcyBub3Qgc3VwcG9ydGVkIGluIENJIGVudmlyb25tZW50JylcbiAgICAgIH0gZWxzZSB7XG4gICAgICAgIGRlYnVnKCdVc2UgJWknLCAnY2FyZ28td2F0Y2gnKVxuICAgICAgICB0cnlJbnN0YWxsQ2FyZ29CaW5hcnkoJ2NhcmdvLXdhdGNoJywgJ3dhdGNoJylcbiAgICAgICAgLy8geWFybiBuYXBpIHdhdGNoIC0tdGFyZ2V0IHg4Nl82NC11bmtub3duLWxpbnV4LWdudSBbLS1jcm9zcy1jb21waWxlXVxuICAgICAgICAvLyA9PT0+XG4gICAgICAgIC8vIGNhcmdvIHdhdGNoIFsuLi5dIC0tIGJ1aWxkIC0tdGFyZ2V0IHg4Nl82NC11bmtub3duLWxpbnV4LWdudVxuICAgICAgICAvLyBjYXJnbyB3YXRjaCBbLi4uXSAtLSB6aWdidWlsZCAtLXRhcmdldCB4ODZfNjQtdW5rbm93bi1saW51eC1nbnVcbiAgICAgICAgdGhpcy5hcmdzLnB1c2goXG4gICAgICAgICAgJ3dhdGNoJyxcbiAgICAgICAgICAnLS13aHknLFxuICAgICAgICAgICctaScsXG4gICAgICAgICAgJyoue2pzLHRzLG5vZGV9JyxcbiAgICAgICAgICAnLXcnLFxuICAgICAgICAgIHRoaXMuY3JhdGVEaXIsXG4gICAgICAgICAgJy0tJyxcbiAgICAgICAgICAnY2FyZ28nLFxuICAgICAgICAgICdidWlsZCcsXG4gICAgICAgIClcbiAgICAgICAgc2V0ID0gdHJ1ZVxuICAgICAgfVxuICAgIH1cblxuICAgIGlmICh0aGlzLm9wdGlvbnMuY3Jvc3NDb21waWxlKSB7XG4gICAgICBpZiAodGhpcy50YXJnZXQucGxhdGZvcm0gPT09ICd3aW4zMicpIHtcbiAgICAgICAgaWYgKHByb2Nlc3MucGxhdGZvcm0gPT09ICd3aW4zMicpIHtcbiAgICAgICAgICBkZWJ1Zy53YXJuKFxuICAgICAgICAgICAgJ1lvdSBhcmUgdHJ5aW5nIHRvIGNyb3NzIGNvbXBpbGUgdG8gd2luMzIgcGxhdGZvcm0gb24gd2luMzIgcGxhdGZvcm0gd2hpY2ggaXMgdW5uZWNlc3NhcnkuJyxcbiAgICAgICAgICApXG4gICAgICAgIH0gZWxzZSB7XG4gICAgICAgICAgLy8gdXNlIGNhcmdvLXh3aW4gdG8gY3Jvc3MgY29tcGlsZSB0byB3aW4zMiBwbGF0Zm9ybVxuICAgICAgICAgIGRlYnVnKCdVc2UgJWknLCAnY2FyZ28teHdpbicpXG4gICAgICAgICAgdHJ5SW5zdGFsbENhcmdvQmluYXJ5KCdjYXJnby14d2luJywgJ3h3aW4nKVxuICAgICAgICAgIHRoaXMuYXJncy5wdXNoKCd4d2luJywgJ2J1aWxkJylcbiAgICAgICAgICBpZiAodGhpcy50YXJnZXQuYXJjaCA9PT0gJ2lhMzInKSB7XG4gICAgICAgICAgICB0aGlzLmVudnMuWFdJTl9BUkNIID0gJ3g4NidcbiAgICAgICAgICB9XG4gICAgICAgICAgc2V0ID0gdHJ1ZVxuICAgICAgICB9XG4gICAgICB9IGVsc2Uge1xuICAgICAgICBpZiAoXG4gICAgICAgICAgdGhpcy50YXJnZXQucGxhdGZvcm0gPT09ICdsaW51eCcgJiZcbiAgICAgICAgICBwcm9jZXNzLnBsYXRmb3JtID09PSAnbGludXgnICYmXG4gICAgICAgICAgdGhpcy50YXJnZXQuYXJjaCA9PT0gcHJvY2Vzcy5hcmNoICYmXG4gICAgICAgICAgKGZ1bmN0aW9uIChhYmk6IHN0cmluZyB8IG51bGwpIHtcbiAgICAgICAgICAgIGNvbnN0IGdsaWJjVmVyc2lvblJ1bnRpbWUgPVxuICAgICAgICAgICAgICAvLyBAdHMtZXhwZWN0LWVycm9yXG4gICAgICAgICAgICAgIHByb2Nlc3MucmVwb3J0Py5nZXRSZXBvcnQoKT8uaGVhZGVyPy5nbGliY1ZlcnNpb25SdW50aW1lXG4gICAgICAgICAgICBjb25zdCBsaWJjID0gZ2xpYmNWZXJzaW9uUnVudGltZSA/ICdnbnUnIDogJ211c2wnXG4gICAgICAgICAgICByZXR1cm4gYWJpID09PSBsaWJjXG4gICAgICAgICAgfSkodGhpcy50YXJnZXQuYWJpKVxuICAgICAgICApIHtcbiAgICAgICAgICBkZWJ1Zy53YXJuKFxuICAgICAgICAgICAgJ1lvdSBhcmUgdHJ5aW5nIHRvIGNyb3NzIGNvbXBpbGUgdG8gbGludXggdGFyZ2V0IG9uIGxpbnV4IHBsYXRmb3JtIHdoaWNoIGlzIHVubmVjZXNzYXJ5LicsXG4gICAgICAgICAgKVxuICAgICAgICB9IGVsc2UgaWYgKFxuICAgICAgICAgIHRoaXMudGFyZ2V0LnBsYXRmb3JtID09PSAnZGFyd2luJyAmJlxuICAgICAgICAgIHByb2Nlc3MucGxhdGZvcm0gPT09ICdkYXJ3aW4nXG4gICAgICAgICkge1xuICAgICAgICAgIGRlYnVnLndhcm4oXG4gICAgICAgICAgICAnWW91IGFyZSB0cnlpbmcgdG8gY3Jvc3MgY29tcGlsZSB0byBkYXJ3aW4gdGFyZ2V0IG9uIGRhcndpbiBwbGF0Zm9ybSB3aGljaCBpcyB1bm5lY2Vzc2FyeS4nLFxuICAgICAgICAgIClcbiAgICAgICAgfSBlbHNlIHtcbiAgICAgICAgICAvLyB1c2UgY2FyZ28temlnYnVpbGQgdG8gY3Jvc3MgY29tcGlsZSB0byBvdGhlciBwbGF0Zm9ybXNcbiAgICAgICAgICBkZWJ1ZygnVXNlICVpJywgJ2NhcmdvLXppZ2J1aWxkJylcbiAgICAgICAgICB0cnlJbnN0YWxsQ2FyZ29CaW5hcnkoJ2NhcmdvLXppZ2J1aWxkJywgJ3ppZ2J1aWxkJylcbiAgICAgICAgICB0aGlzLmFyZ3MucHVzaCgnemlnYnVpbGQnKVxuICAgICAgICAgIHNldCA9IHRydWVcbiAgICAgICAgfVxuICAgICAgfVxuICAgIH1cblxuICAgIGlmICghc2V0KSB7XG4gICAgICB0aGlzLmFyZ3MucHVzaCgnYnVpbGQnKVxuICAgIH1cbiAgICByZXR1cm4gdGhpc1xuICB9XG5cbiAgcHJpdmF0ZSBzZXRQYWNrYWdlKCkge1xuICAgIGNvbnN0IGFyZ3MgPSBbXVxuXG4gICAgaWYgKHRoaXMub3B0aW9ucy5wYWNrYWdlKSB7XG4gICAgICBhcmdzLnB1c2goJy0tcGFja2FnZScsIHRoaXMub3B0aW9ucy5wYWNrYWdlKVxuICAgIH1cblxuICAgIGlmICh0aGlzLmJpbk5hbWUpIHtcbiAgICAgIGFyZ3MucHVzaCgnLS1iaW4nLCB0aGlzLmJpbk5hbWUpXG4gICAgfVxuXG4gICAgaWYgKGFyZ3MubGVuZ3RoKSB7XG4gICAgICBkZWJ1ZygnU2V0IHBhY2thZ2UgZmxhZ3M6ICcpXG4gICAgICBkZWJ1ZygnICAlTycsIGFyZ3MpXG4gICAgICB0aGlzLmFyZ3MucHVzaCguLi5hcmdzKVxuICAgIH1cblxuICAgIHJldHVybiB0aGlzXG4gIH1cblxuICBwcml2YXRlIHNldFRhcmdldCgpIHtcbiAgICBkZWJ1ZygnU2V0IGNvbXBpbGluZyB0YXJnZXQgdG86ICcpXG4gICAgZGVidWcoJyAgJWknLCB0aGlzLnRhcmdldC50cmlwbGUpXG5cbiAgICB0aGlzLmFyZ3MucHVzaCgnLS10YXJnZXQnLCB0aGlzLnRhcmdldC50cmlwbGUpXG5cbiAgICByZXR1cm4gdGhpc1xuICB9XG5cbiAgcHJpdmF0ZSBzZXRFbnZzKCkge1xuICAgIC8vIFRZUEUgREVGXG4gICAgaWYgKHRoaXMuZW5hYmxlVHlwZURlZikge1xuICAgICAgdGhpcy5lbnZzLk5BUElfVFlQRV9ERUZfVE1QX0ZPTERFUiA9XG4gICAgICAgIHRoaXMuZ2VuZXJhdGVJbnRlcm1lZGlhdGVUeXBlRGVmRm9sZGVyKClcbiAgICAgIHRoaXMuc2V0Rm9yY2VCdWlsZEVudnModGhpcy5lbnZzLk5BUElfVFlQRV9ERUZfVE1QX0ZPTERFUilcbiAgICB9XG5cbiAgICAvLyBSVVNURkxBR1NcbiAgICBsZXQgcnVzdGZsYWdzID1cbiAgICAgIHByb2Nlc3MuZW52LlJVU1RGTEFHUyA/PyBwcm9jZXNzLmVudi5DQVJHT19CVUlMRF9SVVNURkxBR1MgPz8gJydcblxuICAgIGlmIChcbiAgICAgIHRoaXMudGFyZ2V0LmFiaT8uaW5jbHVkZXMoJ211c2wnKSAmJlxuICAgICAgIXJ1c3RmbGFncy5pbmNsdWRlcygndGFyZ2V0LWZlYXR1cmU9LWNydC1zdGF0aWMnKVxuICAgICkge1xuICAgICAgcnVzdGZsYWdzICs9ICcgLUMgdGFyZ2V0LWZlYXR1cmU9LWNydC1zdGF0aWMnXG4gICAgfVxuXG4gICAgaWYgKHRoaXMub3B0aW9ucy5zdHJpcCAmJiAhcnVzdGZsYWdzLmluY2x1ZGVzKCdsaW5rLWFyZz0tcycpKSB7XG4gICAgICBydXN0ZmxhZ3MgKz0gJyAtQyBsaW5rLWFyZz0tcydcbiAgICB9XG5cbiAgICBpZiAocnVzdGZsYWdzLmxlbmd0aCkge1xuICAgICAgdGhpcy5lbnZzLlJVU1RGTEFHUyA9IHJ1c3RmbGFnc1xuICAgIH1cbiAgICAvLyBFTkQgUlVTVEZMQUdTXG5cbiAgICAvLyBMSU5LRVJcbiAgICBjb25zdCBsaW5rZXIgPSB0aGlzLm9wdGlvbnMuY3Jvc3NDb21waWxlXG4gICAgICA/IHZvaWQgMFxuICAgICAgOiBnZXRUYXJnZXRMaW5rZXIodGhpcy50YXJnZXQudHJpcGxlKVxuICAgIC8vIFRPRE86XG4gICAgLy8gICBkaXJlY3RseSBzZXQgQ0FSR09fVEFSR0VUXzx0YXJnZXQ+X0xJTktFUiB3aWxsIGNvdmVyIC5jYXJnby9jb25maWcudG9tbFxuICAgIC8vICAgd2lsbCBkZXRlY3QgYnkgY2FyZ28gY29uZmlnIHdoZW4gaXQgYmVjb21lcyBzdGFibGVcbiAgICAvLyAgIHNlZTogaHR0cHM6Ly9naXRodWIuY29tL3J1c3QtbGFuZy9jYXJnby9pc3N1ZXMvOTMwMVxuICAgIGNvbnN0IGxpbmtlckVudiA9IGBDQVJHT19UQVJHRVRfJHt0YXJnZXRUb0VudlZhcihcbiAgICAgIHRoaXMudGFyZ2V0LnRyaXBsZSxcbiAgICApfV9MSU5LRVJgXG4gICAgaWYgKGxpbmtlciAmJiAhcHJvY2Vzcy5lbnZbbGlua2VyRW52XSAmJiAhdGhpcy5lbnZzW2xpbmtlckVudl0pIHtcbiAgICAgIHRoaXMuZW52c1tsaW5rZXJFbnZdID0gbGlua2VyXG4gICAgfVxuXG4gICAgaWYgKHRoaXMudGFyZ2V0LnBsYXRmb3JtID09PSAnYW5kcm9pZCcpIHtcbiAgICAgIHRoaXMuc2V0QW5kcm9pZEVudigpXG4gICAgfVxuXG4gICAgaWYgKHRoaXMudGFyZ2V0LnBsYXRmb3JtID09PSAnd2FzaScpIHtcbiAgICAgIHRoaXMuc2V0V2FzaUVudigpXG4gICAgfVxuXG4gICAgaWYgKHRoaXMudGFyZ2V0LnBsYXRmb3JtID09PSAnb3Blbmhhcm1vbnknKSB7XG4gICAgICB0aGlzLnNldE9wZW5IYXJtb255RW52KClcbiAgICB9XG5cbiAgICBkZWJ1ZygnU2V0IGVudnM6ICcpXG4gICAgT2JqZWN0LmVudHJpZXModGhpcy5lbnZzKS5mb3JFYWNoKChbaywgdl0pID0+IHtcbiAgICAgIGRlYnVnKCcgICVpJywgYCR7a309JHt2fWApXG4gICAgfSlcblxuICAgIHJldHVybiB0aGlzXG4gIH1cblxuICBwcml2YXRlIHNldEZvcmNlQnVpbGRFbnZzKHR5cGVEZWZUbXBGb2xkZXI6IHN0cmluZykge1xuICAgIC8vIGR5bmFtaWNhbGx5IGNoZWNrIGFsbCBuYXBpLXJzIGRlcHMgYW5kIHNldCBgTkFQSV9GT1JDRV9CVUlMRF97dXBwZXJjYXNlKHNuYWtlX2Nhc2UobmFtZSkpfSA9IHRpbWVzdGFtcGBcbiAgICB0aGlzLm1ldGFkYXRhLnBhY2thZ2VzLmZvckVhY2goKGNyYXRlKSA9PiB7XG4gICAgICBpZiAoXG4gICAgICAgIGNyYXRlLmRlcGVuZGVuY2llcy5zb21lKChkKSA9PiBkLm5hbWUgPT09ICduYXBpLWRlcml2ZScpICYmXG4gICAgICAgICFleGlzdHNTeW5jKGpvaW4odHlwZURlZlRtcEZvbGRlciwgY3JhdGUubmFtZSkpXG4gICAgICApIHtcbiAgICAgICAgdGhpcy5lbnZzW1xuICAgICAgICAgIGBOQVBJX0ZPUkNFX0JVSUxEXyR7Y3JhdGUubmFtZS5yZXBsYWNlKC8tL2csICdfJykudG9VcHBlckNhc2UoKX1gXG4gICAgICAgIF0gPSBEYXRlLm5vdygpLnRvU3RyaW5nKClcbiAgICAgIH1cbiAgICB9KVxuICB9XG5cbiAgcHJpdmF0ZSBzZXRBbmRyb2lkRW52KCkge1xuICAgIGNvbnN0IHsgQU5EUk9JRF9OREtfTEFURVNUX0hPTUUgfSA9IHByb2Nlc3MuZW52XG4gICAgaWYgKCFBTkRST0lEX05ES19MQVRFU1RfSE9NRSkge1xuICAgICAgZGVidWcud2FybihcbiAgICAgICAgYCR7Y29sb3JzLnJlZChcbiAgICAgICAgICAnQU5EUk9JRF9OREtfTEFURVNUX0hPTUUnLFxuICAgICAgICApfSBlbnZpcm9ubWVudCB2YXJpYWJsZSBpcyBtaXNzaW5nYCxcbiAgICAgIClcbiAgICB9XG5cbiAgICAvLyBza2lwIGNyb3NzIGNvbXBpbGUgc2V0dXAgaWYgaG9zdCBpcyBhbmRyb2lkXG4gICAgaWYgKHByb2Nlc3MucGxhdGZvcm0gPT09ICdhbmRyb2lkJykge1xuICAgICAgcmV0dXJuXG4gICAgfVxuXG4gICAgY29uc3QgdGFyZ2V0QXJjaCA9IHRoaXMudGFyZ2V0LmFyY2ggPT09ICdhcm0nID8gJ2FybXY3YScgOiAnYWFyY2g2NCdcbiAgICBjb25zdCB0YXJnZXRQbGF0Zm9ybSA9XG4gICAgICB0aGlzLnRhcmdldC5hcmNoID09PSAnYXJtJyA/ICdhbmRyb2lkZWFiaTI0JyA6ICdhbmRyb2lkMjQnXG4gICAgY29uc3QgaG9zdFBsYXRmb3JtID1cbiAgICAgIHByb2Nlc3MucGxhdGZvcm0gPT09ICdkYXJ3aW4nXG4gICAgICAgID8gJ2RhcndpbidcbiAgICAgICAgOiBwcm9jZXNzLnBsYXRmb3JtID09PSAnd2luMzInXG4gICAgICAgICAgPyAnd2luZG93cydcbiAgICAgICAgICA6ICdsaW51eCdcbiAgICBPYmplY3QuYXNzaWduKHRoaXMuZW52cywge1xuICAgICAgQ0FSR09fVEFSR0VUX0FBUkNINjRfTElOVVhfQU5EUk9JRF9MSU5LRVI6IGAke0FORFJPSURfTkRLX0xBVEVTVF9IT01FfS90b29sY2hhaW5zL2xsdm0vcHJlYnVpbHQvJHtob3N0UGxhdGZvcm19LXg4Nl82NC9iaW4vJHt0YXJnZXRBcmNofS1saW51eC1hbmRyb2lkMjQtY2xhbmdgLFxuICAgICAgQ0FSR09fVEFSR0VUX0FSTVY3X0xJTlVYX0FORFJPSURFQUJJX0xJTktFUjogYCR7QU5EUk9JRF9OREtfTEFURVNUX0hPTUV9L3Rvb2xjaGFpbnMvbGx2bS9wcmVidWlsdC8ke2hvc3RQbGF0Zm9ybX0teDg2XzY0L2Jpbi8ke3RhcmdldEFyY2h9LWxpbnV4LWFuZHJvaWRlYWJpMjQtY2xhbmdgLFxuICAgICAgVEFSR0VUX0NDOiBgJHtBTkRST0lEX05ES19MQVRFU1RfSE9NRX0vdG9vbGNoYWlucy9sbHZtL3ByZWJ1aWx0LyR7aG9zdFBsYXRmb3JtfS14ODZfNjQvYmluLyR7dGFyZ2V0QXJjaH0tbGludXgtJHt0YXJnZXRQbGF0Zm9ybX0tY2xhbmdgLFxuICAgICAgVEFSR0VUX0NYWDogYCR7QU5EUk9JRF9OREtfTEFURVNUX0hPTUV9L3Rvb2xjaGFpbnMvbGx2bS9wcmVidWlsdC8ke2hvc3RQbGF0Zm9ybX0teDg2XzY0L2Jpbi8ke3RhcmdldEFyY2h9LWxpbnV4LSR7dGFyZ2V0UGxhdGZvcm19LWNsYW5nKytgLFxuICAgICAgVEFSR0VUX0FSOiBgJHtBTkRST0lEX05ES19MQVRFU1RfSE9NRX0vdG9vbGNoYWlucy9sbHZtL3ByZWJ1aWx0LyR7aG9zdFBsYXRmb3JtfS14ODZfNjQvYmluL2xsdm0tYXJgLFxuICAgICAgVEFSR0VUX1JBTkxJQjogYCR7QU5EUk9JRF9OREtfTEFURVNUX0hPTUV9L3Rvb2xjaGFpbnMvbGx2bS9wcmVidWlsdC8ke2hvc3RQbGF0Zm9ybX0teDg2XzY0L2Jpbi9sbHZtLXJhbmxpYmAsXG4gICAgICBBTkRST0lEX05ESzogQU5EUk9JRF9OREtfTEFURVNUX0hPTUUsXG4gICAgICBQQVRIOiBgJHtBTkRST0lEX05ES19MQVRFU1RfSE9NRX0vdG9vbGNoYWlucy9sbHZtL3ByZWJ1aWx0LyR7aG9zdFBsYXRmb3JtfS14ODZfNjQvYmluJHtwcm9jZXNzLnBsYXRmb3JtID09PSAnd2luMzInID8gJzsnIDogJzonfSR7cHJvY2Vzcy5lbnYuUEFUSH1gLFxuICAgIH0pXG4gIH1cblxuICBwcml2YXRlIHNldFdhc2lFbnYoKSB7XG4gICAgY29uc3QgZW1uYXBpID0gam9pbihcbiAgICAgIHJlcXVpcmUucmVzb2x2ZSgnZW1uYXBpJyksXG4gICAgICAnLi4nLFxuICAgICAgJ2xpYicsXG4gICAgICAnd2FzbTMyLXdhc2ktdGhyZWFkcycsXG4gICAgKVxuICAgIHRoaXMuZW52cy5FTU5BUElfTElOS19ESVIgPSBlbW5hcGlcbiAgICBjb25zdCB7IFdBU0lfU0RLX1BBVEggfSA9IHByb2Nlc3MuZW52XG5cbiAgICBpZiAoV0FTSV9TREtfUEFUSCAmJiBleGlzdHNTeW5jKFdBU0lfU0RLX1BBVEgpKSB7XG4gICAgICB0aGlzLmVudnMuQ0FSR09fVEFSR0VUX1dBU00zMl9XQVNJX1BSRVZJRVcxX1RIUkVBRFNfTElOS0VSID0gam9pbihcbiAgICAgICAgV0FTSV9TREtfUEFUSCxcbiAgICAgICAgJ2JpbicsXG4gICAgICAgICd3YXNtLWxkJyxcbiAgICAgIClcbiAgICAgIHRoaXMuZW52cy5DQVJHT19UQVJHRVRfV0FTTTMyX1dBU0lQMV9MSU5LRVIgPSBqb2luKFxuICAgICAgICBXQVNJX1NES19QQVRILFxuICAgICAgICAnYmluJyxcbiAgICAgICAgJ3dhc20tbGQnLFxuICAgICAgKVxuICAgICAgdGhpcy5lbnZzLkNBUkdPX1RBUkdFVF9XQVNNMzJfV0FTSVAxX1RIUkVBRFNfTElOS0VSID0gam9pbihcbiAgICAgICAgV0FTSV9TREtfUEFUSCxcbiAgICAgICAgJ2JpbicsXG4gICAgICAgICd3YXNtLWxkJyxcbiAgICAgIClcbiAgICAgIHRoaXMuZW52cy5DQVJHT19UQVJHRVRfV0FTTTMyX1dBU0lQMl9MSU5LRVIgPSBqb2luKFxuICAgICAgICBXQVNJX1NES19QQVRILFxuICAgICAgICAnYmluJyxcbiAgICAgICAgJ3dhc20tbGQnLFxuICAgICAgKVxuICAgICAgdGhpcy5zZXRFbnZJZk5vdEV4aXN0cygnVEFSR0VUX0NDJywgam9pbihXQVNJX1NES19QQVRILCAnYmluJywgJ2NsYW5nJykpXG4gICAgICB0aGlzLnNldEVudklmTm90RXhpc3RzKFxuICAgICAgICAnVEFSR0VUX0NYWCcsXG4gICAgICAgIGpvaW4oV0FTSV9TREtfUEFUSCwgJ2JpbicsICdjbGFuZysrJyksXG4gICAgICApXG4gICAgICB0aGlzLnNldEVudklmTm90RXhpc3RzKCdUQVJHRVRfQVInLCBqb2luKFdBU0lfU0RLX1BBVEgsICdiaW4nLCAnYXInKSlcbiAgICAgIHRoaXMuc2V0RW52SWZOb3RFeGlzdHMoXG4gICAgICAgICdUQVJHRVRfUkFOTElCJyxcbiAgICAgICAgam9pbihXQVNJX1NES19QQVRILCAnYmluJywgJ3JhbmxpYicpLFxuICAgICAgKVxuICAgICAgdGhpcy5zZXRFbnZJZk5vdEV4aXN0cyhcbiAgICAgICAgJ1RBUkdFVF9DRkxBR1MnLFxuICAgICAgICBgLS10YXJnZXQ9d2FzbTMyLXdhc2ktdGhyZWFkcyAtLXN5c3Jvb3Q9JHtXQVNJX1NES19QQVRIfS9zaGFyZS93YXNpLXN5c3Jvb3QgLXB0aHJlYWQgLW1sbHZtIC13YXNtLWVuYWJsZS1zamxqYCxcbiAgICAgIClcbiAgICAgIHRoaXMuc2V0RW52SWZOb3RFeGlzdHMoXG4gICAgICAgICdUQVJHRVRfQ1hYRkxBR1MnLFxuICAgICAgICBgLS10YXJnZXQ9d2FzbTMyLXdhc2ktdGhyZWFkcyAtLXN5c3Jvb3Q9JHtXQVNJX1NES19QQVRIfS9zaGFyZS93YXNpLXN5c3Jvb3QgLXB0aHJlYWQgLW1sbHZtIC13YXNtLWVuYWJsZS1zamxqYCxcbiAgICAgIClcbiAgICAgIHRoaXMuc2V0RW52SWZOb3RFeGlzdHMoXG4gICAgICAgIGBUQVJHRVRfTERGTEFHU2AsXG4gICAgICAgIGAtZnVzZS1sZD0ke1dBU0lfU0RLX1BBVEh9L2Jpbi93YXNtLWxkIC0tdGFyZ2V0PXdhc20zMi13YXNpLXRocmVhZHNgLFxuICAgICAgKVxuICAgIH1cbiAgfVxuXG4gIHByaXZhdGUgc2V0T3Blbkhhcm1vbnlFbnYoKSB7XG4gICAgY29uc3QgeyBPSE9TX1NES19QQVRILCBPSE9TX1NES19OQVRJVkUgfSA9IHByb2Nlc3MuZW52XG4gICAgY29uc3QgbmRrUGF0aCA9IE9IT1NfU0RLX1BBVEggPyBgJHtPSE9TX1NES19QQVRIfS9uYXRpdmVgIDogT0hPU19TREtfTkFUSVZFXG4gICAgLy8gQHRzLWV4cGVjdC1lcnJvclxuICAgIGlmICghbmRrUGF0aCAmJiBwcm9jZXNzLnBsYXRmb3JtICE9PSAnb3Blbmhhcm1vbnknKSB7XG4gICAgICBkZWJ1Zy53YXJuKFxuICAgICAgICBgJHtjb2xvcnMucmVkKCdPSE9TX1NES19QQVRIJyl9IG9yICR7Y29sb3JzLnJlZCgnT0hPU19TREtfTkFUSVZFJyl9IGVudmlyb25tZW50IHZhcmlhYmxlIGlzIG1pc3NpbmdgLFxuICAgICAgKVxuICAgICAgcmV0dXJuXG4gICAgfVxuICAgIGNvbnN0IGxpbmtlck5hbWUgPSBgQ0FSR09fVEFSR0VUXyR7dGhpcy50YXJnZXQudHJpcGxlLnRvVXBwZXJDYXNlKCkucmVwbGFjZSgvLS9nLCAnXycpfV9MSU5LRVJgXG4gICAgY29uc3QgcmFuUGF0aCA9IGAke25ka1BhdGh9L2xsdm0vYmluL2xsdm0tcmFubGliYFxuICAgIGNvbnN0IGFyUGF0aCA9IGAke25ka1BhdGh9L2xsdm0vYmluL2xsdm0tYXJgXG4gICAgY29uc3QgY2NQYXRoID0gYCR7bmRrUGF0aH0vbGx2bS9iaW4vJHt0aGlzLnRhcmdldC50cmlwbGV9LWNsYW5nYFxuICAgIGNvbnN0IGN4eFBhdGggPSBgJHtuZGtQYXRofS9sbHZtL2Jpbi8ke3RoaXMudGFyZ2V0LnRyaXBsZX0tY2xhbmcrK2BcbiAgICBjb25zdCBhc1BhdGggPSBgJHtuZGtQYXRofS9sbHZtL2Jpbi9sbHZtLWFzYFxuICAgIGNvbnN0IGxkUGF0aCA9IGAke25ka1BhdGh9L2xsdm0vYmluL2xkLmxsZGBcbiAgICBjb25zdCBzdHJpcFBhdGggPSBgJHtuZGtQYXRofS9sbHZtL2Jpbi9sbHZtLXN0cmlwYFxuICAgIGNvbnN0IG9iakR1bXBQYXRoID0gYCR7bmRrUGF0aH0vbGx2bS9iaW4vbGx2bS1vYmpkdW1wYFxuICAgIGNvbnN0IG9iakNvcHlQYXRoID0gYCR7bmRrUGF0aH0vbGx2bS9iaW4vbGx2bS1vYmpjb3B5YFxuICAgIGNvbnN0IG5tUGF0aCA9IGAke25ka1BhdGh9L2xsdm0vYmluL2xsdm0tbm1gXG4gICAgY29uc3QgYmluUGF0aCA9IGAke25ka1BhdGh9L2xsdm0vYmluYFxuICAgIGNvbnN0IGxpYlBhdGggPSBgJHtuZGtQYXRofS9sbHZtL2xpYmBcblxuICAgIHRoaXMuc2V0RW52SWZOb3RFeGlzdHMoJ0xJQkNMQU5HX1BBVEgnLCBsaWJQYXRoKVxuICAgIHRoaXMuc2V0RW52SWZOb3RFeGlzdHMoJ0RFUF9BVE9NSUMnLCAnY2xhbmdfcnQuYnVpbHRpbnMnKVxuICAgIHRoaXMuc2V0RW52SWZOb3RFeGlzdHMobGlua2VyTmFtZSwgY2NQYXRoKVxuICAgIHRoaXMuc2V0RW52SWZOb3RFeGlzdHMoJ1RBUkdFVF9DQycsIGNjUGF0aClcbiAgICB0aGlzLnNldEVudklmTm90RXhpc3RzKCdUQVJHRVRfQ1hYJywgY3h4UGF0aClcbiAgICB0aGlzLnNldEVudklmTm90RXhpc3RzKCdUQVJHRVRfQVInLCBhclBhdGgpXG4gICAgdGhpcy5zZXRFbnZJZk5vdEV4aXN0cygnVEFSR0VUX1JBTkxJQicsIHJhblBhdGgpXG4gICAgdGhpcy5zZXRFbnZJZk5vdEV4aXN0cygnVEFSR0VUX0FTJywgYXNQYXRoKVxuICAgIHRoaXMuc2V0RW52SWZOb3RFeGlzdHMoJ1RBUkdFVF9MRCcsIGxkUGF0aClcbiAgICB0aGlzLnNldEVudklmTm90RXhpc3RzKCdUQVJHRVRfU1RSSVAnLCBzdHJpcFBhdGgpXG4gICAgdGhpcy5zZXRFbnZJZk5vdEV4aXN0cygnVEFSR0VUX09CSkRVTVAnLCBvYmpEdW1wUGF0aClcbiAgICB0aGlzLnNldEVudklmTm90RXhpc3RzKCdUQVJHRVRfT0JKQ09QWScsIG9iakNvcHlQYXRoKVxuICAgIHRoaXMuc2V0RW52SWZOb3RFeGlzdHMoJ1RBUkdFVF9OTScsIG5tUGF0aClcbiAgICB0aGlzLmVudnMuUEFUSCA9IGAke2JpblBhdGh9JHtwcm9jZXNzLnBsYXRmb3JtID09PSAnd2luMzInID8gJzsnIDogJzonfSR7cHJvY2Vzcy5lbnYuUEFUSH1gXG4gIH1cblxuICBwcml2YXRlIHNldEZlYXR1cmVzKCkge1xuICAgIGNvbnN0IGFyZ3MgPSBbXVxuICAgIGlmICh0aGlzLm9wdGlvbnMuYWxsRmVhdHVyZXMgJiYgdGhpcy5vcHRpb25zLm5vRGVmYXVsdEZlYXR1cmVzKSB7XG4gICAgICB0aHJvdyBuZXcgRXJyb3IoXG4gICAgICAgICdDYW5ub3Qgc3BlY2lmeSAtLWFsbC1mZWF0dXJlcyBhbmQgLS1uby1kZWZhdWx0LWZlYXR1cmVzIHRvZ2V0aGVyJyxcbiAgICAgIClcbiAgICB9XG4gICAgaWYgKHRoaXMub3B0aW9ucy5hbGxGZWF0dXJlcykge1xuICAgICAgYXJncy5wdXNoKCctLWFsbC1mZWF0dXJlcycpXG4gICAgfSBlbHNlIGlmICh0aGlzLm9wdGlvbnMubm9EZWZhdWx0RmVhdHVyZXMpIHtcbiAgICAgIGFyZ3MucHVzaCgnLS1uby1kZWZhdWx0LWZlYXR1cmVzJylcbiAgICB9XG4gICAgaWYgKHRoaXMub3B0aW9ucy5mZWF0dXJlcykge1xuICAgICAgYXJncy5wdXNoKCctLWZlYXR1cmVzJywgLi4udGhpcy5vcHRpb25zLmZlYXR1cmVzKVxuICAgIH1cblxuICAgIGRlYnVnKCdTZXQgZmVhdHVyZXMgZmxhZ3M6ICcpXG4gICAgZGVidWcoJyAgJU8nLCBhcmdzKVxuICAgIHRoaXMuYXJncy5wdXNoKC4uLmFyZ3MpXG5cbiAgICByZXR1cm4gdGhpc1xuICB9XG5cbiAgcHJpdmF0ZSBzZXRCeXBhc3NBcmdzKCkge1xuICAgIGlmICh0aGlzLm9wdGlvbnMucmVsZWFzZSkge1xuICAgICAgdGhpcy5hcmdzLnB1c2goJy0tcmVsZWFzZScpXG4gICAgfVxuXG4gICAgaWYgKHRoaXMub3B0aW9ucy52ZXJib3NlKSB7XG4gICAgICB0aGlzLmFyZ3MucHVzaCgnLS12ZXJib3NlJylcbiAgICB9XG5cbiAgICBpZiAodGhpcy5vcHRpb25zLnRhcmdldERpcikge1xuICAgICAgdGhpcy5hcmdzLnB1c2goJy0tdGFyZ2V0LWRpcicsIHRoaXMub3B0aW9ucy50YXJnZXREaXIpXG4gICAgfVxuXG4gICAgaWYgKHRoaXMub3B0aW9ucy5wcm9maWxlKSB7XG4gICAgICB0aGlzLmFyZ3MucHVzaCgnLS1wcm9maWxlJywgdGhpcy5vcHRpb25zLnByb2ZpbGUpXG4gICAgfVxuXG4gICAgaWYgKHRoaXMub3B0aW9ucy5tYW5pZmVzdFBhdGgpIHtcbiAgICAgIHRoaXMuYXJncy5wdXNoKCctLW1hbmlmZXN0LXBhdGgnLCB0aGlzLm9wdGlvbnMubWFuaWZlc3RQYXRoKVxuICAgIH1cblxuICAgIGlmICh0aGlzLm9wdGlvbnMuY2FyZ29PcHRpb25zPy5sZW5ndGgpIHtcbiAgICAgIHRoaXMuYXJncy5wdXNoKC4uLnRoaXMub3B0aW9ucy5jYXJnb09wdGlvbnMpXG4gICAgfVxuXG4gICAgcmV0dXJuIHRoaXNcbiAgfVxuXG4gIHByaXZhdGUgZ2VuZXJhdGVJbnRlcm1lZGlhdGVUeXBlRGVmRm9sZGVyKCkge1xuICAgIGxldCBmb2xkZXIgPSBqb2luKFxuICAgICAgdGhpcy50YXJnZXREaXIsXG4gICAgICAnbmFwaS1ycycsXG4gICAgICBgJHt0aGlzLmNyYXRlLm5hbWV9LSR7Y3JlYXRlSGFzaCgnc2hhMjU2JylcbiAgICAgICAgLnVwZGF0ZSh0aGlzLmNyYXRlLm1hbmlmZXN0X3BhdGgpXG4gICAgICAgIC51cGRhdGUoQ0xJX1ZFUlNJT04pXG4gICAgICAgIC5kaWdlc3QoJ2hleCcpXG4gICAgICAgIC5zdWJzdHJpbmcoMCwgOCl9YCxcbiAgICApXG5cbiAgICBpZiAoIXRoaXMub3B0aW9ucy5kdHNDYWNoZSkge1xuICAgICAgcm1TeW5jKGZvbGRlciwgeyByZWN1cnNpdmU6IHRydWUsIGZvcmNlOiB0cnVlIH0pXG4gICAgICBmb2xkZXIgKz0gYF8ke0RhdGUubm93KCl9YFxuICAgIH1cblxuICAgIG1rZGlyQXN5bmMoZm9sZGVyLCB7IHJlY3Vyc2l2ZTogdHJ1ZSB9KVxuXG4gICAgcmV0dXJuIGZvbGRlclxuICB9XG5cbiAgcHJpdmF0ZSBhc3luYyBwb3N0QnVpbGQoKSB7XG4gICAgdHJ5IHtcbiAgICAgIGRlYnVnKGBUcnkgdG8gY3JlYXRlIG91dHB1dCBkaXJlY3Rvcnk6YClcbiAgICAgIGRlYnVnKCcgICVpJywgdGhpcy5vdXRwdXREaXIpXG4gICAgICBhd2FpdCBta2RpckFzeW5jKHRoaXMub3V0cHV0RGlyLCB7IHJlY3Vyc2l2ZTogdHJ1ZSB9KVxuICAgICAgZGVidWcoYE91dHB1dCBkaXJlY3RvcnkgY3JlYXRlZGApXG4gICAgfSBjYXRjaCAoZSkge1xuICAgICAgdGhyb3cgbmV3IEVycm9yKGBGYWlsZWQgdG8gY3JlYXRlIG91dHB1dCBkaXJlY3RvcnkgJHt0aGlzLm91dHB1dERpcn1gLCB7XG4gICAgICAgIGNhdXNlOiBlLFxuICAgICAgfSlcbiAgICB9XG5cbiAgICBjb25zdCB3YXNtQmluYXJ5TmFtZSA9IGF3YWl0IHRoaXMuY29weUFydGlmYWN0KClcblxuICAgIC8vIG9ubHkgZm9yIGNkeWxpYlxuICAgIGlmICh0aGlzLmNkeUxpYk5hbWUpIHtcbiAgICAgIGNvbnN0IGlkZW50cyA9IGF3YWl0IHRoaXMuZ2VuZXJhdGVUeXBlRGVmKClcbiAgICAgIGNvbnN0IGpzT3V0cHV0ID0gYXdhaXQgdGhpcy53cml0ZUpzQmluZGluZyhpZGVudHMpXG4gICAgICBjb25zdCB3YXNtQmluZGluZ3NPdXRwdXQgPSBhd2FpdCB0aGlzLndyaXRlV2FzaUJpbmRpbmcoXG4gICAgICAgIHdhc21CaW5hcnlOYW1lLFxuICAgICAgICBpZGVudHMsXG4gICAgICApXG4gICAgICBpZiAoanNPdXRwdXQpIHtcbiAgICAgICAgdGhpcy5vdXRwdXRzLnB1c2goanNPdXRwdXQpXG4gICAgICB9XG4gICAgICBpZiAod2FzbUJpbmRpbmdzT3V0cHV0KSB7XG4gICAgICAgIHRoaXMub3V0cHV0cy5wdXNoKC4uLndhc21CaW5kaW5nc091dHB1dClcbiAgICAgIH1cbiAgICB9XG5cbiAgICByZXR1cm4gdGhpcy5vdXRwdXRzXG4gIH1cblxuICBwcml2YXRlIGFzeW5jIGNvcHlBcnRpZmFjdCgpIHtcbiAgICBjb25zdCBbc3JjTmFtZSwgZGVzdE5hbWUsIHdhc21CaW5hcnlOYW1lXSA9IHRoaXMuZ2V0QXJ0aWZhY3ROYW1lcygpXG4gICAgaWYgKCFzcmNOYW1lIHx8ICFkZXN0TmFtZSkge1xuICAgICAgcmV0dXJuXG4gICAgfVxuXG4gICAgY29uc3QgcHJvZmlsZSA9XG4gICAgICB0aGlzLm9wdGlvbnMucHJvZmlsZSA/PyAodGhpcy5vcHRpb25zLnJlbGVhc2UgPyAncmVsZWFzZScgOiAnZGVidWcnKVxuICAgIGNvbnN0IHNyYyA9IGpvaW4odGhpcy50YXJnZXREaXIsIHRoaXMudGFyZ2V0LnRyaXBsZSwgcHJvZmlsZSwgc3JjTmFtZSlcbiAgICBkZWJ1ZyhgQ29weSBhcnRpZmFjdCBmcm9tOiBbJHtzcmN9XWApXG4gICAgY29uc3QgZGVzdCA9IGpvaW4odGhpcy5vdXRwdXREaXIsIGRlc3ROYW1lKVxuICAgIGNvbnN0IGlzV2FzbSA9IGRlc3QuZW5kc1dpdGgoJy53YXNtJylcblxuICAgIHRyeSB7XG4gICAgICBpZiAoYXdhaXQgZmlsZUV4aXN0cyhkZXN0KSkge1xuICAgICAgICBkZWJ1ZygnT2xkIGFydGlmYWN0IGZvdW5kLCByZW1vdmUgaXQgZmlyc3QnKVxuICAgICAgICBhd2FpdCB1bmxpbmtBc3luYyhkZXN0KVxuICAgICAgfVxuICAgICAgZGVidWcoJ0NvcHkgYXJ0aWZhY3QgdG86JylcbiAgICAgIGRlYnVnKCcgICVpJywgZGVzdClcbiAgICAgIGlmIChpc1dhc20pIHtcbiAgICAgICAgY29uc3QgeyBNb2R1bGVDb25maWcgfSA9IGF3YWl0IGltcG9ydCgnQG5hcGktcnMvd2FzbS10b29scycpXG4gICAgICAgIGRlYnVnKCdHZW5lcmF0ZSBkZWJ1ZyB3YXNtIG1vZHVsZScpXG4gICAgICAgIHRyeSB7XG4gICAgICAgICAgY29uc3QgZGVidWdXYXNtTW9kdWxlID0gbmV3IE1vZHVsZUNvbmZpZygpXG4gICAgICAgICAgICAuZ2VuZXJhdGVEd2FyZih0cnVlKVxuICAgICAgICAgICAgLmdlbmVyYXRlTmFtZVNlY3Rpb24odHJ1ZSlcbiAgICAgICAgICAgIC5nZW5lcmF0ZVByb2R1Y2Vyc1NlY3Rpb24odHJ1ZSlcbiAgICAgICAgICAgIC5wcmVzZXJ2ZUNvZGVUcmFuc2Zvcm0odHJ1ZSlcbiAgICAgICAgICAgIC5zdHJpY3RWYWxpZGF0ZShmYWxzZSlcbiAgICAgICAgICAgIC5wYXJzZShhd2FpdCByZWFkRmlsZUFzeW5jKHNyYykpXG4gICAgICAgICAgY29uc3QgZGVidWdXYXNtQmluYXJ5ID0gZGVidWdXYXNtTW9kdWxlLmVtaXRXYXNtKHRydWUpXG4gICAgICAgICAgYXdhaXQgd3JpdGVGaWxlQXN5bmMoXG4gICAgICAgICAgICBkZXN0LnJlcGxhY2UoL1xcLndhc20kLywgJy5kZWJ1Zy53YXNtJyksXG4gICAgICAgICAgICBkZWJ1Z1dhc21CaW5hcnksXG4gICAgICAgICAgKVxuICAgICAgICAgIGRlYnVnKCdHZW5lcmF0ZSByZWxlYXNlIHdhc20gbW9kdWxlJylcbiAgICAgICAgICBjb25zdCByZWxlYXNlV2FzbU1vZHVsZSA9IG5ldyBNb2R1bGVDb25maWcoKVxuICAgICAgICAgICAgLmdlbmVyYXRlRHdhcmYoZmFsc2UpXG4gICAgICAgICAgICAuZ2VuZXJhdGVOYW1lU2VjdGlvbihmYWxzZSlcbiAgICAgICAgICAgIC5nZW5lcmF0ZVByb2R1Y2Vyc1NlY3Rpb24oZmFsc2UpXG4gICAgICAgICAgICAucHJlc2VydmVDb2RlVHJhbnNmb3JtKGZhbHNlKVxuICAgICAgICAgICAgLnN0cmljdFZhbGlkYXRlKGZhbHNlKVxuICAgICAgICAgICAgLm9ubHlTdGFibGVGZWF0dXJlcyhmYWxzZSlcbiAgICAgICAgICAgIC5wYXJzZShkZWJ1Z1dhc21CaW5hcnkpXG4gICAgICAgICAgY29uc3QgcmVsZWFzZVdhc21CaW5hcnkgPSByZWxlYXNlV2FzbU1vZHVsZS5lbWl0V2FzbShmYWxzZSlcbiAgICAgICAgICBhd2FpdCB3cml0ZUZpbGVBc3luYyhkZXN0LCByZWxlYXNlV2FzbUJpbmFyeSlcbiAgICAgICAgfSBjYXRjaCAoZSkge1xuICAgICAgICAgIGRlYnVnLndhcm4oXG4gICAgICAgICAgICBgRmFpbGVkIHRvIGdlbmVyYXRlIGRlYnVnIHdhc20gbW9kdWxlOiAkeyhlIGFzIGFueSkubWVzc2FnZSA/PyBlfWAsXG4gICAgICAgICAgKVxuICAgICAgICAgIGF3YWl0IGNvcHlGaWxlQXN5bmMoc3JjLCBkZXN0KVxuICAgICAgICB9XG4gICAgICB9IGVsc2Uge1xuICAgICAgICBhd2FpdCBjb3B5RmlsZUFzeW5jKHNyYywgZGVzdClcbiAgICAgIH1cbiAgICAgIHRoaXMub3V0cHV0cy5wdXNoKHtcbiAgICAgICAga2luZDogZGVzdC5lbmRzV2l0aCgnLm5vZGUnKSA/ICdub2RlJyA6IGlzV2FzbSA/ICd3YXNtJyA6ICdleGUnLFxuICAgICAgICBwYXRoOiBkZXN0LFxuICAgICAgfSlcbiAgICAgIHJldHVybiB3YXNtQmluYXJ5TmFtZSA/IGpvaW4odGhpcy5vdXRwdXREaXIsIHdhc21CaW5hcnlOYW1lKSA6IG51bGxcbiAgICB9IGNhdGNoIChlKSB7XG4gICAgICB0aHJvdyBuZXcgRXJyb3IoJ0ZhaWxlZCB0byBjb3B5IGFydGlmYWN0JywgeyBjYXVzZTogZSB9KVxuICAgIH1cbiAgfVxuXG4gIHByaXZhdGUgZ2V0QXJ0aWZhY3ROYW1lcygpIHtcbiAgICBpZiAodGhpcy5jZHlMaWJOYW1lKSB7XG4gICAgICBjb25zdCBjZHlMaWIgPSB0aGlzLmNkeUxpYk5hbWUucmVwbGFjZSgvLS9nLCAnXycpXG4gICAgICBjb25zdCB3YXNpVGFyZ2V0ID0gdGhpcy5jb25maWcudGFyZ2V0cy5maW5kKCh0KSA9PiB0LnBsYXRmb3JtID09PSAnd2FzaScpXG5cbiAgICAgIGNvbnN0IHNyY05hbWUgPVxuICAgICAgICB0aGlzLnRhcmdldC5wbGF0Zm9ybSA9PT0gJ2RhcndpbidcbiAgICAgICAgICA/IGBsaWIke2NkeUxpYn0uZHlsaWJgXG4gICAgICAgICAgOiB0aGlzLnRhcmdldC5wbGF0Zm9ybSA9PT0gJ3dpbjMyJ1xuICAgICAgICAgICAgPyBgJHtjZHlMaWJ9LmRsbGBcbiAgICAgICAgICAgIDogdGhpcy50YXJnZXQucGxhdGZvcm0gPT09ICd3YXNpJyB8fCB0aGlzLnRhcmdldC5wbGF0Zm9ybSA9PT0gJ3dhc20nXG4gICAgICAgICAgICAgID8gYCR7Y2R5TGlifS53YXNtYFxuICAgICAgICAgICAgICA6IGBsaWIke2NkeUxpYn0uc29gXG5cbiAgICAgIGxldCBkZXN0TmFtZSA9IHRoaXMuY29uZmlnLmJpbmFyeU5hbWVcbiAgICAgIC8vIGFkZCBwbGF0Zm9ybSBzdWZmaXggdG8gYmluYXJ5IG5hbWVcbiAgICAgIC8vIGluZGV4Wy5saW51eC14NjQtZ251XS5ub2RlXG4gICAgICAvLyAgICAgICBeXl5eXl5eXl5eXl5eXlxuICAgICAgaWYgKHRoaXMub3B0aW9ucy5wbGF0Zm9ybSkge1xuICAgICAgICBkZXN0TmFtZSArPSBgLiR7dGhpcy50YXJnZXQucGxhdGZvcm1BcmNoQUJJfWBcbiAgICAgIH1cbiAgICAgIGlmIChzcmNOYW1lLmVuZHNXaXRoKCcud2FzbScpKSB7XG4gICAgICAgIGRlc3ROYW1lICs9ICcud2FzbSdcbiAgICAgIH0gZWxzZSB7XG4gICAgICAgIGRlc3ROYW1lICs9ICcubm9kZSdcbiAgICAgIH1cblxuICAgICAgcmV0dXJuIFtcbiAgICAgICAgc3JjTmFtZSxcbiAgICAgICAgZGVzdE5hbWUsXG4gICAgICAgIHdhc2lUYXJnZXRcbiAgICAgICAgICA/IGAke3RoaXMuY29uZmlnLmJpbmFyeU5hbWV9LiR7d2FzaVRhcmdldC5wbGF0Zm9ybUFyY2hBQkl9Lndhc21gXG4gICAgICAgICAgOiBudWxsLFxuICAgICAgXVxuICAgIH0gZWxzZSBpZiAodGhpcy5iaW5OYW1lKSB7XG4gICAgICBjb25zdCBzcmNOYW1lID1cbiAgICAgICAgdGhpcy50YXJnZXQucGxhdGZvcm0gPT09ICd3aW4zMicgPyBgJHt0aGlzLmJpbk5hbWV9LmV4ZWAgOiB0aGlzLmJpbk5hbWVcblxuICAgICAgcmV0dXJuIFtzcmNOYW1lLCBzcmNOYW1lXVxuICAgIH1cblxuICAgIHJldHVybiBbXVxuICB9XG5cbiAgcHJpdmF0ZSBhc3luYyBnZW5lcmF0ZVR5cGVEZWYoKSB7XG4gICAgY29uc3QgdHlwZURlZkRpciA9IHRoaXMuZW52cy5OQVBJX1RZUEVfREVGX1RNUF9GT0xERVJcbiAgICBpZiAoIXRoaXMuZW5hYmxlVHlwZURlZikge1xuICAgICAgcmV0dXJuIFtdXG4gICAgfVxuXG4gICAgY29uc3QgeyBleHBvcnRzLCBkdHMgfSA9IGF3YWl0IGdlbmVyYXRlVHlwZURlZih7XG4gICAgICB0eXBlRGVmRGlyLFxuICAgICAgbm9EdHNIZWFkZXI6IHRoaXMub3B0aW9ucy5ub0R0c0hlYWRlcixcbiAgICAgIGR0c0hlYWRlcjogdGhpcy5vcHRpb25zLmR0c0hlYWRlcixcbiAgICAgIGNvbmZpZ0R0c0hlYWRlcjogdGhpcy5jb25maWcuZHRzSGVhZGVyLFxuICAgICAgY29uZmlnRHRzSGVhZGVyRmlsZTogdGhpcy5jb25maWcuZHRzSGVhZGVyRmlsZSxcbiAgICAgIGNvbnN0RW51bTogdGhpcy5vcHRpb25zLmNvbnN0RW51bSA/PyB0aGlzLmNvbmZpZy5jb25zdEVudW0sXG4gICAgICBjd2Q6IHRoaXMub3B0aW9ucy5jd2QsXG4gICAgfSlcblxuICAgIGNvbnN0IGRlc3QgPSBqb2luKHRoaXMub3V0cHV0RGlyLCB0aGlzLm9wdGlvbnMuZHRzID8/ICdpbmRleC5kLnRzJylcblxuICAgIHRyeSB7XG4gICAgICBkZWJ1ZygnV3JpdGluZyB0eXBlIGRlZiB0bzonKVxuICAgICAgZGVidWcoJyAgJWknLCBkZXN0KVxuICAgICAgYXdhaXQgd3JpdGVGaWxlQXN5bmMoZGVzdCwgZHRzLCAndXRmLTgnKVxuICAgIH0gY2F0Y2ggKGUpIHtcbiAgICAgIGRlYnVnLmVycm9yKCdGYWlsZWQgdG8gd3JpdGUgdHlwZSBkZWYgZmlsZScpXG4gICAgICBkZWJ1Zy5lcnJvcihlIGFzIEVycm9yKVxuICAgIH1cblxuICAgIGlmIChleHBvcnRzLmxlbmd0aCA+IDApIHtcbiAgICAgIGNvbnN0IGRlc3QgPSBqb2luKHRoaXMub3V0cHV0RGlyLCB0aGlzLm9wdGlvbnMuZHRzID8/ICdpbmRleC5kLnRzJylcbiAgICAgIHRoaXMub3V0cHV0cy5wdXNoKHsga2luZDogJ2R0cycsIHBhdGg6IGRlc3QgfSlcbiAgICB9XG5cbiAgICByZXR1cm4gZXhwb3J0c1xuICB9XG5cbiAgcHJpdmF0ZSBhc3luYyB3cml0ZUpzQmluZGluZyhpZGVudHM6IHN0cmluZ1tdKSB7XG4gICAgcmV0dXJuIHdyaXRlSnNCaW5kaW5nKHtcbiAgICAgIHBsYXRmb3JtOiB0aGlzLm9wdGlvbnMucGxhdGZvcm0sXG4gICAgICBub0pzQmluZGluZzogdGhpcy5vcHRpb25zLm5vSnNCaW5kaW5nLFxuICAgICAgaWRlbnRzLFxuICAgICAganNCaW5kaW5nOiB0aGlzLm9wdGlvbnMuanNCaW5kaW5nLFxuICAgICAgZXNtOiB0aGlzLm9wdGlvbnMuZXNtLFxuICAgICAgYmluYXJ5TmFtZTogdGhpcy5jb25maWcuYmluYXJ5TmFtZSxcbiAgICAgIHBhY2thZ2VOYW1lOiB0aGlzLm9wdGlvbnMuanNQYWNrYWdlTmFtZSA/PyB0aGlzLmNvbmZpZy5wYWNrYWdlTmFtZSxcbiAgICAgIHZlcnNpb246IHByb2Nlc3MuZW52Lm5wbV9uZXdfdmVyc2lvbiA/PyB0aGlzLmNvbmZpZy5wYWNrYWdlSnNvbi52ZXJzaW9uLFxuICAgICAgb3V0cHV0RGlyOiB0aGlzLm91dHB1dERpcixcbiAgICB9KVxuICB9XG5cbiAgcHJpdmF0ZSBhc3luYyB3cml0ZVdhc2lCaW5kaW5nKFxuICAgIGRpc3RGaWxlTmFtZTogc3RyaW5nIHwgdW5kZWZpbmVkIHwgbnVsbCxcbiAgICBpZGVudHM6IHN0cmluZ1tdLFxuICApIHtcbiAgICBpZiAoZGlzdEZpbGVOYW1lKSB7XG4gICAgICBjb25zdCB7IG5hbWUsIGRpciB9ID0gcGFyc2UoZGlzdEZpbGVOYW1lKVxuICAgICAgY29uc3QgYmluZGluZ1BhdGggPSBqb2luKGRpciwgYCR7dGhpcy5jb25maWcuYmluYXJ5TmFtZX0ud2FzaS5janNgKVxuICAgICAgY29uc3QgYnJvd3NlckJpbmRpbmdQYXRoID0gam9pbihcbiAgICAgICAgZGlyLFxuICAgICAgICBgJHt0aGlzLmNvbmZpZy5iaW5hcnlOYW1lfS53YXNpLWJyb3dzZXIuanNgLFxuICAgICAgKVxuICAgICAgY29uc3Qgd29ya2VyUGF0aCA9IGpvaW4oZGlyLCAnd2FzaS13b3JrZXIubWpzJylcbiAgICAgIGNvbnN0IGJyb3dzZXJXb3JrZXJQYXRoID0gam9pbihkaXIsICd3YXNpLXdvcmtlci1icm93c2VyLm1qcycpXG4gICAgICBjb25zdCBicm93c2VyRW50cnlQYXRoID0gam9pbihkaXIsICdicm93c2VyLmpzJylcbiAgICAgIGNvbnN0IGV4cG9ydHNDb2RlID1cbiAgICAgICAgYG1vZHVsZS5leHBvcnRzID0gX19uYXBpTW9kdWxlLmV4cG9ydHNcXG5gICtcbiAgICAgICAgaWRlbnRzXG4gICAgICAgICAgLm1hcChcbiAgICAgICAgICAgIChpZGVudCkgPT5cbiAgICAgICAgICAgICAgYG1vZHVsZS5leHBvcnRzLiR7aWRlbnR9ID0gX19uYXBpTW9kdWxlLmV4cG9ydHMuJHtpZGVudH1gLFxuICAgICAgICAgIClcbiAgICAgICAgICAuam9pbignXFxuJylcbiAgICAgIGF3YWl0IHdyaXRlRmlsZUFzeW5jKFxuICAgICAgICBiaW5kaW5nUGF0aCxcbiAgICAgICAgY3JlYXRlV2FzaUJpbmRpbmcoXG4gICAgICAgICAgbmFtZSxcbiAgICAgICAgICB0aGlzLmNvbmZpZy5wYWNrYWdlTmFtZSxcbiAgICAgICAgICB0aGlzLmNvbmZpZy53YXNtPy5pbml0aWFsTWVtb3J5LFxuICAgICAgICAgIHRoaXMuY29uZmlnLndhc20/Lm1heGltdW1NZW1vcnksXG4gICAgICAgICkgK1xuICAgICAgICAgIGV4cG9ydHNDb2RlICtcbiAgICAgICAgICAnXFxuJyxcbiAgICAgICAgJ3V0ZjgnLFxuICAgICAgKVxuICAgICAgYXdhaXQgd3JpdGVGaWxlQXN5bmMoXG4gICAgICAgIGJyb3dzZXJCaW5kaW5nUGF0aCxcbiAgICAgICAgY3JlYXRlV2FzaUJyb3dzZXJCaW5kaW5nKFxuICAgICAgICAgIG5hbWUsXG4gICAgICAgICAgdGhpcy5jb25maWcud2FzbT8uaW5pdGlhbE1lbW9yeSxcbiAgICAgICAgICB0aGlzLmNvbmZpZy53YXNtPy5tYXhpbXVtTWVtb3J5LFxuICAgICAgICAgIHRoaXMuY29uZmlnLndhc20/LmJyb3dzZXI/LmZzLFxuICAgICAgICAgIHRoaXMuY29uZmlnLndhc20/LmJyb3dzZXI/LmFzeW5jSW5pdCxcbiAgICAgICAgICB0aGlzLmNvbmZpZy53YXNtPy5icm93c2VyPy5idWZmZXIsXG4gICAgICAgICkgK1xuICAgICAgICAgIGBleHBvcnQgZGVmYXVsdCBfX25hcGlNb2R1bGUuZXhwb3J0c1xcbmAgK1xuICAgICAgICAgIGlkZW50c1xuICAgICAgICAgICAgLm1hcChcbiAgICAgICAgICAgICAgKGlkZW50KSA9PlxuICAgICAgICAgICAgICAgIGBleHBvcnQgY29uc3QgJHtpZGVudH0gPSBfX25hcGlNb2R1bGUuZXhwb3J0cy4ke2lkZW50fWAsXG4gICAgICAgICAgICApXG4gICAgICAgICAgICAuam9pbignXFxuJykgK1xuICAgICAgICAgICdcXG4nLFxuICAgICAgICAndXRmOCcsXG4gICAgICApXG4gICAgICBhd2FpdCB3cml0ZUZpbGVBc3luYyh3b3JrZXJQYXRoLCBXQVNJX1dPUktFUl9URU1QTEFURSwgJ3V0ZjgnKVxuICAgICAgYXdhaXQgd3JpdGVGaWxlQXN5bmMoXG4gICAgICAgIGJyb3dzZXJXb3JrZXJQYXRoLFxuICAgICAgICBjcmVhdGVXYXNpQnJvd3NlcldvcmtlckJpbmRpbmcodGhpcy5jb25maWcud2FzbT8uYnJvd3Nlcj8uZnMgPz8gZmFsc2UpLFxuICAgICAgICAndXRmOCcsXG4gICAgICApXG4gICAgICBhd2FpdCB3cml0ZUZpbGVBc3luYyhcbiAgICAgICAgYnJvd3NlckVudHJ5UGF0aCxcbiAgICAgICAgYGV4cG9ydCAqIGZyb20gJyR7dGhpcy5jb25maWcucGFja2FnZU5hbWV9LXdhc20zMi13YXNpJ1xcbmAsXG4gICAgICApXG4gICAgICByZXR1cm4gW1xuICAgICAgICB7IGtpbmQ6ICdqcycsIHBhdGg6IGJpbmRpbmdQYXRoIH0sXG4gICAgICAgIHsga2luZDogJ2pzJywgcGF0aDogYnJvd3NlckJpbmRpbmdQYXRoIH0sXG4gICAgICAgIHsga2luZDogJ2pzJywgcGF0aDogd29ya2VyUGF0aCB9LFxuICAgICAgICB7IGtpbmQ6ICdqcycsIHBhdGg6IGJyb3dzZXJXb3JrZXJQYXRoIH0sXG4gICAgICAgIHsga2luZDogJ2pzJywgcGF0aDogYnJvd3NlckVudHJ5UGF0aCB9LFxuICAgICAgXSBzYXRpc2ZpZXMgT3V0cHV0W11cbiAgICB9XG4gICAgcmV0dXJuIFtdXG4gIH1cblxuICBwcml2YXRlIHNldEVudklmTm90RXhpc3RzKGVudjogc3RyaW5nLCB2YWx1ZTogc3RyaW5nKSB7XG4gICAgaWYgKCFwcm9jZXNzLmVudltlbnZdKSB7XG4gICAgICB0aGlzLmVudnNbZW52XSA9IHZhbHVlXG4gICAgfVxuICB9XG59XG5cbmV4cG9ydCBpbnRlcmZhY2UgV3JpdGVKc0JpbmRpbmdPcHRpb25zIHtcbiAgcGxhdGZvcm0/OiBib29sZWFuXG4gIG5vSnNCaW5kaW5nPzogYm9vbGVhblxuICBpZGVudHM6IHN0cmluZ1tdXG4gIGpzQmluZGluZz86IHN0cmluZ1xuICBlc20/OiBib29sZWFuXG4gIGJpbmFyeU5hbWU6IHN0cmluZ1xuICBwYWNrYWdlTmFtZTogc3RyaW5nXG4gIHZlcnNpb246IHN0cmluZ1xuICBvdXRwdXREaXI6IHN0cmluZ1xufVxuXG5leHBvcnQgYXN5bmMgZnVuY3Rpb24gd3JpdGVKc0JpbmRpbmcoXG4gIG9wdGlvbnM6IFdyaXRlSnNCaW5kaW5nT3B0aW9ucyxcbik6IFByb21pc2U8T3V0cHV0IHwgdW5kZWZpbmVkPiB7XG4gIGlmIChcbiAgICAhb3B0aW9ucy5wbGF0Zm9ybSB8fFxuICAgIC8vIGVzbGludC1kaXNhYmxlLW5leHQtbGluZSBAdHlwZXNjcmlwdC1lc2xpbnQvcHJlZmVyLW51bGxpc2gtY29hbGVzY2luZ1xuICAgIG9wdGlvbnMubm9Kc0JpbmRpbmcgfHxcbiAgICBvcHRpb25zLmlkZW50cy5sZW5ndGggPT09IDBcbiAgKSB7XG4gICAgcmV0dXJuXG4gIH1cblxuICBjb25zdCBuYW1lID0gb3B0aW9ucy5qc0JpbmRpbmcgPz8gJ2luZGV4LmpzJ1xuXG4gIGNvbnN0IGNyZWF0ZUJpbmRpbmcgPSBvcHRpb25zLmVzbSA/IGNyZWF0ZUVzbUJpbmRpbmcgOiBjcmVhdGVDanNCaW5kaW5nXG4gIGNvbnN0IGJpbmRpbmcgPSBjcmVhdGVCaW5kaW5nKFxuICAgIG9wdGlvbnMuYmluYXJ5TmFtZSxcbiAgICBvcHRpb25zLnBhY2thZ2VOYW1lLFxuICAgIG9wdGlvbnMuaWRlbnRzLFxuICAgIC8vIGluIG5wbSBwcmV2ZXJzaW9uIGhvb2tcbiAgICBvcHRpb25zLnZlcnNpb24sXG4gIClcblxuICB0cnkge1xuICAgIGNvbnN0IGRlc3QgPSBqb2luKG9wdGlvbnMub3V0cHV0RGlyLCBuYW1lKVxuICAgIGRlYnVnKCdXcml0aW5nIGpzIGJpbmRpbmcgdG86JylcbiAgICBkZWJ1ZygnICAlaScsIGRlc3QpXG4gICAgYXdhaXQgd3JpdGVGaWxlQXN5bmMoZGVzdCwgYmluZGluZywgJ3V0Zi04JylcbiAgICByZXR1cm4geyBraW5kOiAnanMnLCBwYXRoOiBkZXN0IH0gc2F0aXNmaWVzIE91dHB1dFxuICB9IGNhdGNoIChlKSB7XG4gICAgdGhyb3cgbmV3IEVycm9yKCdGYWlsZWQgdG8gd3JpdGUganMgYmluZGluZyBmaWxlJywgeyBjYXVzZTogZSB9KVxuICB9XG59XG5cbmV4cG9ydCBpbnRlcmZhY2UgR2VuZXJhdGVUeXBlRGVmT3B0aW9ucyB7XG4gIHR5cGVEZWZEaXI6IHN0cmluZ1xuICBub0R0c0hlYWRlcj86IGJvb2xlYW5cbiAgZHRzSGVhZGVyPzogc3RyaW5nXG4gIGR0c0hlYWRlckZpbGU/OiBzdHJpbmdcbiAgY29uZmlnRHRzSGVhZGVyPzogc3RyaW5nXG4gIGNvbmZpZ0R0c0hlYWRlckZpbGU/OiBzdHJpbmdcbiAgY29uc3RFbnVtPzogYm9vbGVhblxuICBjd2Q6IHN0cmluZ1xufVxuXG5leHBvcnQgYXN5bmMgZnVuY3Rpb24gZ2VuZXJhdGVUeXBlRGVmKFxuICBvcHRpb25zOiBHZW5lcmF0ZVR5cGVEZWZPcHRpb25zLFxuKTogUHJvbWlzZTx7IGV4cG9ydHM6IHN0cmluZ1tdOyBkdHM6IHN0cmluZyB9PiB7XG4gIGlmICghKGF3YWl0IGRpckV4aXN0c0FzeW5jKG9wdGlvbnMudHlwZURlZkRpcikpKSB7XG4gICAgcmV0dXJuIHsgZXhwb3J0czogW10sIGR0czogJycgfVxuICB9XG5cbiAgbGV0IGhlYWRlciA9ICcnXG4gIGxldCBkdHMgPSAnJ1xuICBsZXQgZXhwb3J0czogc3RyaW5nW10gPSBbXVxuXG4gIGlmICghb3B0aW9ucy5ub0R0c0hlYWRlcikge1xuICAgIGNvbnN0IGR0c0hlYWRlciA9IG9wdGlvbnMuZHRzSGVhZGVyID8/IG9wdGlvbnMuY29uZmlnRHRzSGVhZGVyXG4gICAgLy8gYGR0c0hlYWRlckZpbGVgIGluIGNvbmZpZyA+IGBkdHNIZWFkZXJgIGluIGNsaSBmbGFnID4gYGR0c0hlYWRlcmAgaW4gY29uZmlnXG4gICAgaWYgKG9wdGlvbnMuY29uZmlnRHRzSGVhZGVyRmlsZSkge1xuICAgICAgdHJ5IHtcbiAgICAgICAgaGVhZGVyID0gYXdhaXQgcmVhZEZpbGVBc3luYyhcbiAgICAgICAgICBqb2luKG9wdGlvbnMuY3dkLCBvcHRpb25zLmNvbmZpZ0R0c0hlYWRlckZpbGUpLFxuICAgICAgICAgICd1dGYtOCcsXG4gICAgICAgIClcbiAgICAgIH0gY2F0Y2ggKGUpIHtcbiAgICAgICAgZGVidWcud2FybihcbiAgICAgICAgICBgRmFpbGVkIHRvIHJlYWQgZHRzIGhlYWRlciBmaWxlICR7b3B0aW9ucy5jb25maWdEdHNIZWFkZXJGaWxlfWAsXG4gICAgICAgICAgZSxcbiAgICAgICAgKVxuICAgICAgfVxuICAgIH0gZWxzZSBpZiAoZHRzSGVhZGVyKSB7XG4gICAgICBoZWFkZXIgPSBkdHNIZWFkZXJcbiAgICB9IGVsc2Uge1xuICAgICAgaGVhZGVyID0gREVGQVVMVF9UWVBFX0RFRl9IRUFERVJcbiAgICB9XG4gIH1cblxuICBjb25zdCBmaWxlcyA9IGF3YWl0IHJlYWRkaXJBc3luYyhvcHRpb25zLnR5cGVEZWZEaXIsIHsgd2l0aEZpbGVUeXBlczogdHJ1ZSB9KVxuXG4gIGlmICghZmlsZXMubGVuZ3RoKSB7XG4gICAgZGVidWcoJ05vIHR5cGUgZGVmIGZpbGVzIGZvdW5kLiBTa2lwIGdlbmVyYXRpbmcgZHRzIGZpbGUuJylcbiAgICByZXR1cm4geyBleHBvcnRzOiBbXSwgZHRzOiAnJyB9XG4gIH1cblxuICBmb3IgKGNvbnN0IGZpbGUgb2YgZmlsZXMpIHtcbiAgICBpZiAoIWZpbGUuaXNGaWxlKCkpIHtcbiAgICAgIGNvbnRpbnVlXG4gICAgfVxuXG4gICAgY29uc3QgeyBkdHM6IGZpbGVEdHMsIGV4cG9ydHM6IGZpbGVFeHBvcnRzIH0gPSBhd2FpdCBwcm9jZXNzVHlwZURlZihcbiAgICAgIGpvaW4ob3B0aW9ucy50eXBlRGVmRGlyLCBmaWxlLm5hbWUpLFxuICAgICAgb3B0aW9ucy5jb25zdEVudW0gPz8gdHJ1ZSxcbiAgICApXG5cbiAgICBkdHMgKz0gZmlsZUR0c1xuICAgIGV4cG9ydHMucHVzaCguLi5maWxlRXhwb3J0cylcbiAgfVxuXG4gIGlmIChkdHMuaW5kZXhPZignRXh0ZXJuYWxPYmplY3Q8JykgPiAtMSkge1xuICAgIGhlYWRlciArPSBgXG5leHBvcnQgZGVjbGFyZSBjbGFzcyBFeHRlcm5hbE9iamVjdDxUPiB7XG4gIHJlYWRvbmx5ICcnOiB7XG4gICAgcmVhZG9ubHkgJyc6IHVuaXF1ZSBzeW1ib2xcbiAgICBbSzogc3ltYm9sXTogVFxuICB9XG59XG5gXG4gIH1cblxuICBpZiAoZHRzLmluZGV4T2YoJ1R5cGVkQXJyYXknKSA+IC0xKSB7XG4gICAgaGVhZGVyICs9IGBcbmV4cG9ydCB0eXBlIFR5cGVkQXJyYXkgPSBJbnQ4QXJyYXkgfCBVaW50OEFycmF5IHwgVWludDhDbGFtcGVkQXJyYXkgfCBJbnQxNkFycmF5IHwgVWludDE2QXJyYXkgfCBJbnQzMkFycmF5IHwgVWludDMyQXJyYXkgfCBGbG9hdDMyQXJyYXkgfCBGbG9hdDY0QXJyYXkgfCBCaWdJbnQ2NEFycmF5IHwgQmlnVWludDY0QXJyYXlcbmBcbiAgfVxuXG4gIGR0cyA9IGhlYWRlciArIGR0c1xuXG4gIHJldHVybiB7XG4gICAgZXhwb3J0cyxcbiAgICBkdHMsXG4gIH1cbn1cbiIsIi8vIFRoaXMgZmlsZSBpcyBnZW5lcmF0ZWQgYnkgY29kZWdlbi9pbmRleC50c1xuLy8gRG8gbm90IGVkaXQgdGhpcyBmaWxlIG1hbnVhbGx5XG5pbXBvcnQgeyBDb21tYW5kLCBPcHRpb24gfSBmcm9tICdjbGlwYW5pb24nXG5cbmV4cG9ydCBhYnN0cmFjdCBjbGFzcyBCYXNlQ3JlYXRlTnBtRGlyc0NvbW1hbmQgZXh0ZW5kcyBDb21tYW5kIHtcbiAgc3RhdGljIHBhdGhzID0gW1snY3JlYXRlLW5wbS1kaXJzJ11dXG5cbiAgc3RhdGljIHVzYWdlID0gQ29tbWFuZC5Vc2FnZSh7XG4gICAgZGVzY3JpcHRpb246ICdDcmVhdGUgbnBtIHBhY2thZ2UgZGlycyBmb3IgZGlmZmVyZW50IHBsYXRmb3JtcycsXG4gIH0pXG5cbiAgY3dkID0gT3B0aW9uLlN0cmluZygnLS1jd2QnLCBwcm9jZXNzLmN3ZCgpLCB7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnVGhlIHdvcmtpbmcgZGlyZWN0b3J5IG9mIHdoZXJlIG5hcGkgY29tbWFuZCB3aWxsIGJlIGV4ZWN1dGVkIGluLCBhbGwgb3RoZXIgcGF0aHMgb3B0aW9ucyBhcmUgcmVsYXRpdmUgdG8gdGhpcyBwYXRoJyxcbiAgfSlcblxuICBjb25maWdQYXRoPzogc3RyaW5nID0gT3B0aW9uLlN0cmluZygnLS1jb25maWctcGF0aCwtYycsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1BhdGggdG8gYG5hcGlgIGNvbmZpZyBqc29uIGZpbGUnLFxuICB9KVxuXG4gIHBhY2thZ2VKc29uUGF0aCA9IE9wdGlvbi5TdHJpbmcoJy0tcGFja2FnZS1qc29uLXBhdGgnLCAncGFja2FnZS5qc29uJywge1xuICAgIGRlc2NyaXB0aW9uOiAnUGF0aCB0byBgcGFja2FnZS5qc29uYCcsXG4gIH0pXG5cbiAgbnBtRGlyID0gT3B0aW9uLlN0cmluZygnLS1ucG0tZGlyJywgJ25wbScsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1BhdGggdG8gdGhlIGZvbGRlciB3aGVyZSB0aGUgbnBtIHBhY2thZ2VzIHB1dCcsXG4gIH0pXG5cbiAgZHJ5UnVuID0gT3B0aW9uLkJvb2xlYW4oJy0tZHJ5LXJ1bicsIGZhbHNlLCB7XG4gICAgZGVzY3JpcHRpb246ICdEcnkgcnVuIHdpdGhvdXQgdG91Y2hpbmcgZmlsZSBzeXN0ZW0nLFxuICB9KVxuXG4gIGdldE9wdGlvbnMoKSB7XG4gICAgcmV0dXJuIHtcbiAgICAgIGN3ZDogdGhpcy5jd2QsXG4gICAgICBjb25maWdQYXRoOiB0aGlzLmNvbmZpZ1BhdGgsXG4gICAgICBwYWNrYWdlSnNvblBhdGg6IHRoaXMucGFja2FnZUpzb25QYXRoLFxuICAgICAgbnBtRGlyOiB0aGlzLm5wbURpcixcbiAgICAgIGRyeVJ1bjogdGhpcy5kcnlSdW4sXG4gICAgfVxuICB9XG59XG5cbi8qKlxuICogQ3JlYXRlIG5wbSBwYWNrYWdlIGRpcnMgZm9yIGRpZmZlcmVudCBwbGF0Zm9ybXNcbiAqL1xuZXhwb3J0IGludGVyZmFjZSBDcmVhdGVOcG1EaXJzT3B0aW9ucyB7XG4gIC8qKlxuICAgKiBUaGUgd29ya2luZyBkaXJlY3Rvcnkgb2Ygd2hlcmUgbmFwaSBjb21tYW5kIHdpbGwgYmUgZXhlY3V0ZWQgaW4sIGFsbCBvdGhlciBwYXRocyBvcHRpb25zIGFyZSByZWxhdGl2ZSB0byB0aGlzIHBhdGhcbiAgICpcbiAgICogQGRlZmF1bHQgcHJvY2Vzcy5jd2QoKVxuICAgKi9cbiAgY3dkPzogc3RyaW5nXG4gIC8qKlxuICAgKiBQYXRoIHRvIGBuYXBpYCBjb25maWcganNvbiBmaWxlXG4gICAqL1xuICBjb25maWdQYXRoPzogc3RyaW5nXG4gIC8qKlxuICAgKiBQYXRoIHRvIGBwYWNrYWdlLmpzb25gXG4gICAqXG4gICAqIEBkZWZhdWx0ICdwYWNrYWdlLmpzb24nXG4gICAqL1xuICBwYWNrYWdlSnNvblBhdGg/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gdGhlIGZvbGRlciB3aGVyZSB0aGUgbnBtIHBhY2thZ2VzIHB1dFxuICAgKlxuICAgKiBAZGVmYXVsdCAnbnBtJ1xuICAgKi9cbiAgbnBtRGlyPzogc3RyaW5nXG4gIC8qKlxuICAgKiBEcnkgcnVuIHdpdGhvdXQgdG91Y2hpbmcgZmlsZSBzeXN0ZW1cbiAgICpcbiAgICogQGRlZmF1bHQgZmFsc2VcbiAgICovXG4gIGRyeVJ1bj86IGJvb2xlYW5cbn1cblxuZXhwb3J0IGZ1bmN0aW9uIGFwcGx5RGVmYXVsdENyZWF0ZU5wbURpcnNPcHRpb25zKFxuICBvcHRpb25zOiBDcmVhdGVOcG1EaXJzT3B0aW9ucyxcbikge1xuICByZXR1cm4ge1xuICAgIGN3ZDogcHJvY2Vzcy5jd2QoKSxcbiAgICBwYWNrYWdlSnNvblBhdGg6ICdwYWNrYWdlLmpzb24nLFxuICAgIG5wbURpcjogJ25wbScsXG4gICAgZHJ5UnVuOiBmYWxzZSxcbiAgICAuLi5vcHRpb25zLFxuICB9XG59XG4iLCJpbXBvcnQgeyBqb2luLCByZXNvbHZlIH0gZnJvbSAnbm9kZTpwYXRoJ1xuXG5pbXBvcnQgeyBwYXJzZSB9IGZyb20gJ3NlbXZlcidcblxuaW1wb3J0IHtcbiAgYXBwbHlEZWZhdWx0Q3JlYXRlTnBtRGlyc09wdGlvbnMsXG4gIHR5cGUgQ3JlYXRlTnBtRGlyc09wdGlvbnMsXG59IGZyb20gJy4uL2RlZi9jcmVhdGUtbnBtLWRpcnMuanMnXG5pbXBvcnQge1xuICBkZWJ1Z0ZhY3RvcnksXG4gIHJlYWROYXBpQ29uZmlnLFxuICBta2RpckFzeW5jIGFzIHJhd01rZGlyQXN5bmMsXG4gIHBpY2ssXG4gIHdyaXRlRmlsZUFzeW5jIGFzIHJhd1dyaXRlRmlsZUFzeW5jLFxuICB0eXBlIFRhcmdldCxcbiAgdHlwZSBDb21tb25QYWNrYWdlSnNvbkZpZWxkcyxcbn0gZnJvbSAnLi4vdXRpbHMvaW5kZXguanMnXG5cbmNvbnN0IGRlYnVnID0gZGVidWdGYWN0b3J5KCdjcmVhdGUtbnBtLWRpcnMnKVxuXG5leHBvcnQgaW50ZXJmYWNlIFBhY2thZ2VNZXRhIHtcbiAgJ2Rpc3QtdGFncyc6IHsgW2luZGV4OiBzdHJpbmddOiBzdHJpbmcgfVxufVxuXG5leHBvcnQgYXN5bmMgZnVuY3Rpb24gY3JlYXRlTnBtRGlycyh1c2VyT3B0aW9uczogQ3JlYXRlTnBtRGlyc09wdGlvbnMpIHtcbiAgY29uc3Qgb3B0aW9ucyA9IGFwcGx5RGVmYXVsdENyZWF0ZU5wbURpcnNPcHRpb25zKHVzZXJPcHRpb25zKVxuXG4gIGFzeW5jIGZ1bmN0aW9uIG1rZGlyQXN5bmMoZGlyOiBzdHJpbmcpIHtcbiAgICBkZWJ1ZygnVHJ5IHRvIGNyZWF0ZSBkaXI6ICVpJywgZGlyKVxuICAgIGlmIChvcHRpb25zLmRyeVJ1bikge1xuICAgICAgcmV0dXJuXG4gICAgfVxuXG4gICAgYXdhaXQgcmF3TWtkaXJBc3luYyhkaXIsIHtcbiAgICAgIHJlY3Vyc2l2ZTogdHJ1ZSxcbiAgICB9KVxuICB9XG5cbiAgYXN5bmMgZnVuY3Rpb24gd3JpdGVGaWxlQXN5bmMoZmlsZTogc3RyaW5nLCBjb250ZW50OiBzdHJpbmcpIHtcbiAgICBkZWJ1ZygnV3JpdGluZyBmaWxlICVpJywgZmlsZSlcblxuICAgIGlmIChvcHRpb25zLmRyeVJ1bikge1xuICAgICAgZGVidWcoY29udGVudClcbiAgICAgIHJldHVyblxuICAgIH1cblxuICAgIGF3YWl0IHJhd1dyaXRlRmlsZUFzeW5jKGZpbGUsIGNvbnRlbnQpXG4gIH1cblxuICBjb25zdCBwYWNrYWdlSnNvblBhdGggPSByZXNvbHZlKG9wdGlvbnMuY3dkLCBvcHRpb25zLnBhY2thZ2VKc29uUGF0aClcbiAgY29uc3QgbnBtUGF0aCA9IHJlc29sdmUob3B0aW9ucy5jd2QsIG9wdGlvbnMubnBtRGlyKVxuXG4gIGRlYnVnKGBSZWFkIGNvbnRlbnQgZnJvbSBbJHtvcHRpb25zLmNvbmZpZ1BhdGggPz8gcGFja2FnZUpzb25QYXRofV1gKVxuXG4gIGNvbnN0IHsgdGFyZ2V0cywgYmluYXJ5TmFtZSwgcGFja2FnZU5hbWUsIHBhY2thZ2VKc29uIH0gPVxuICAgIGF3YWl0IHJlYWROYXBpQ29uZmlnKFxuICAgICAgcGFja2FnZUpzb25QYXRoLFxuICAgICAgb3B0aW9ucy5jb25maWdQYXRoID8gcmVzb2x2ZShvcHRpb25zLmN3ZCwgb3B0aW9ucy5jb25maWdQYXRoKSA6IHVuZGVmaW5lZCxcbiAgICApXG5cbiAgZm9yIChjb25zdCB0YXJnZXQgb2YgdGFyZ2V0cykge1xuICAgIGNvbnN0IHRhcmdldERpciA9IGpvaW4obnBtUGF0aCwgYCR7dGFyZ2V0LnBsYXRmb3JtQXJjaEFCSX1gKVxuICAgIGF3YWl0IG1rZGlyQXN5bmModGFyZ2V0RGlyKVxuXG4gICAgY29uc3QgYmluYXJ5RmlsZU5hbWUgPVxuICAgICAgdGFyZ2V0LmFyY2ggPT09ICd3YXNtMzInXG4gICAgICAgID8gYCR7YmluYXJ5TmFtZX0uJHt0YXJnZXQucGxhdGZvcm1BcmNoQUJJfS53YXNtYFxuICAgICAgICA6IGAke2JpbmFyeU5hbWV9LiR7dGFyZ2V0LnBsYXRmb3JtQXJjaEFCSX0ubm9kZWBcbiAgICBjb25zdCBzY29wZWRQYWNrYWdlSnNvbjogQ29tbW9uUGFja2FnZUpzb25GaWVsZHMgPSB7XG4gICAgICBuYW1lOiBgJHtwYWNrYWdlTmFtZX0tJHt0YXJnZXQucGxhdGZvcm1BcmNoQUJJfWAsXG4gICAgICB2ZXJzaW9uOiBwYWNrYWdlSnNvbi52ZXJzaW9uLFxuICAgICAgY3B1OiB0YXJnZXQuYXJjaCAhPT0gJ3VuaXZlcnNhbCcgPyBbdGFyZ2V0LmFyY2hdIDogdW5kZWZpbmVkLFxuICAgICAgbWFpbjogYmluYXJ5RmlsZU5hbWUsXG4gICAgICBmaWxlczogW2JpbmFyeUZpbGVOYW1lXSxcbiAgICAgIC4uLnBpY2soXG4gICAgICAgIHBhY2thZ2VKc29uLFxuICAgICAgICAnZGVzY3JpcHRpb24nLFxuICAgICAgICAna2V5d29yZHMnLFxuICAgICAgICAnYXV0aG9yJyxcbiAgICAgICAgJ2F1dGhvcnMnLFxuICAgICAgICAnaG9tZXBhZ2UnLFxuICAgICAgICAnbGljZW5zZScsXG4gICAgICAgICdlbmdpbmVzJyxcbiAgICAgICAgJ3JlcG9zaXRvcnknLFxuICAgICAgICAnYnVncycsXG4gICAgICApLFxuICAgIH1cbiAgICBpZiAocGFja2FnZUpzb24ucHVibGlzaENvbmZpZykge1xuICAgICAgc2NvcGVkUGFja2FnZUpzb24ucHVibGlzaENvbmZpZyA9IHBpY2soXG4gICAgICAgIHBhY2thZ2VKc29uLnB1Ymxpc2hDb25maWcsXG4gICAgICAgICdyZWdpc3RyeScsXG4gICAgICAgICdhY2Nlc3MnLFxuICAgICAgKVxuICAgIH1cbiAgICBpZiAodGFyZ2V0LmFyY2ggIT09ICd3YXNtMzInKSB7XG4gICAgICBzY29wZWRQYWNrYWdlSnNvbi5vcyA9IFt0YXJnZXQucGxhdGZvcm1dXG4gICAgfSBlbHNlIHtcbiAgICAgIGNvbnN0IGVudHJ5ID0gYCR7YmluYXJ5TmFtZX0ud2FzaS5janNgXG4gICAgICBzY29wZWRQYWNrYWdlSnNvbi5tYWluID0gZW50cnlcbiAgICAgIHNjb3BlZFBhY2thZ2VKc29uLmJyb3dzZXIgPSBgJHtiaW5hcnlOYW1lfS53YXNpLWJyb3dzZXIuanNgXG4gICAgICBzY29wZWRQYWNrYWdlSnNvbi5maWxlcz8ucHVzaChcbiAgICAgICAgZW50cnksXG4gICAgICAgIHNjb3BlZFBhY2thZ2VKc29uLmJyb3dzZXIsXG4gICAgICAgIGB3YXNpLXdvcmtlci5tanNgLFxuICAgICAgICBgd2FzaS13b3JrZXItYnJvd3Nlci5tanNgLFxuICAgICAgKVxuICAgICAgbGV0IG5lZWRSZXN0cmljdE5vZGVWZXJzaW9uID0gdHJ1ZVxuICAgICAgaWYgKHNjb3BlZFBhY2thZ2VKc29uLmVuZ2luZXM/Lm5vZGUpIHtcbiAgICAgICAgdHJ5IHtcbiAgICAgICAgICBjb25zdCB7IG1ham9yIH0gPSBwYXJzZShzY29wZWRQYWNrYWdlSnNvbi5lbmdpbmVzLm5vZGUpID8/IHtcbiAgICAgICAgICAgIG1ham9yOiAwLFxuICAgICAgICAgIH1cbiAgICAgICAgICBpZiAobWFqb3IgPj0gMTQpIHtcbiAgICAgICAgICAgIG5lZWRSZXN0cmljdE5vZGVWZXJzaW9uID0gZmFsc2VcbiAgICAgICAgICB9XG4gICAgICAgIH0gY2F0Y2gge1xuICAgICAgICAgIC8vIGlnbm9yZVxuICAgICAgICB9XG4gICAgICB9XG4gICAgICBpZiAobmVlZFJlc3RyaWN0Tm9kZVZlcnNpb24pIHtcbiAgICAgICAgc2NvcGVkUGFja2FnZUpzb24uZW5naW5lcyA9IHtcbiAgICAgICAgICBub2RlOiAnPj0xNC4wLjAnLFxuICAgICAgICB9XG4gICAgICB9XG4gICAgICBjb25zdCB3YXNtUnVudGltZSA9IGF3YWl0IGZldGNoKFxuICAgICAgICBgaHR0cHM6Ly9yZWdpc3RyeS5ucG1qcy5vcmcvQG5hcGktcnMvd2FzbS1ydW50aW1lYCxcbiAgICAgICkudGhlbigocmVzKSA9PiByZXMuanNvbigpIGFzIFByb21pc2U8UGFja2FnZU1ldGE+KVxuICAgICAgc2NvcGVkUGFja2FnZUpzb24uZGVwZW5kZW5jaWVzID0ge1xuICAgICAgICAnQG5hcGktcnMvd2FzbS1ydW50aW1lJzogYF4ke3dhc21SdW50aW1lWydkaXN0LXRhZ3MnXS5sYXRlc3R9YCxcbiAgICAgIH1cbiAgICB9XG5cbiAgICBpZiAodGFyZ2V0LmFiaSA9PT0gJ2dudScpIHtcbiAgICAgIHNjb3BlZFBhY2thZ2VKc29uLmxpYmMgPSBbJ2dsaWJjJ11cbiAgICB9IGVsc2UgaWYgKHRhcmdldC5hYmkgPT09ICdtdXNsJykge1xuICAgICAgc2NvcGVkUGFja2FnZUpzb24ubGliYyA9IFsnbXVzbCddXG4gICAgfVxuXG4gICAgY29uc3QgdGFyZ2V0UGFja2FnZUpzb24gPSBqb2luKHRhcmdldERpciwgJ3BhY2thZ2UuanNvbicpXG4gICAgYXdhaXQgd3JpdGVGaWxlQXN5bmMoXG4gICAgICB0YXJnZXRQYWNrYWdlSnNvbixcbiAgICAgIEpTT04uc3RyaW5naWZ5KHNjb3BlZFBhY2thZ2VKc29uLCBudWxsLCAyKSArICdcXG4nLFxuICAgIClcbiAgICBjb25zdCB0YXJnZXRSZWFkbWUgPSBqb2luKHRhcmdldERpciwgJ1JFQURNRS5tZCcpXG4gICAgYXdhaXQgd3JpdGVGaWxlQXN5bmModGFyZ2V0UmVhZG1lLCByZWFkbWUocGFja2FnZU5hbWUsIHRhcmdldCkpXG5cbiAgICBkZWJ1Zy5pbmZvKGAke3BhY2thZ2VOYW1lfSAtJHt0YXJnZXQucGxhdGZvcm1BcmNoQUJJfSBjcmVhdGVkYClcbiAgfVxufVxuXG5mdW5jdGlvbiByZWFkbWUocGFja2FnZU5hbWU6IHN0cmluZywgdGFyZ2V0OiBUYXJnZXQpIHtcbiAgcmV0dXJuIGAjIFxcYCR7cGFja2FnZU5hbWV9LSR7dGFyZ2V0LnBsYXRmb3JtQXJjaEFCSX1cXGBcblxuVGhpcyBpcyB0aGUgKioke3RhcmdldC50cmlwbGV9KiogYmluYXJ5IGZvciBcXGAke3BhY2thZ2VOYW1lfVxcYFxuYFxufVxuIiwiLy8gVGhpcyBmaWxlIGlzIGdlbmVyYXRlZCBieSBjb2RlZ2VuL2luZGV4LnRzXG4vLyBEbyBub3QgZWRpdCB0aGlzIGZpbGUgbWFudWFsbHlcbmltcG9ydCB7IENvbW1hbmQsIE9wdGlvbiB9IGZyb20gJ2NsaXBhbmlvbidcbmltcG9ydCAqIGFzIHR5cGFuaW9uIGZyb20gJ3R5cGFuaW9uJ1xuXG5leHBvcnQgYWJzdHJhY3QgY2xhc3MgQmFzZU5ld0NvbW1hbmQgZXh0ZW5kcyBDb21tYW5kIHtcbiAgc3RhdGljIHBhdGhzID0gW1snbmV3J11dXG5cbiAgc3RhdGljIHVzYWdlID0gQ29tbWFuZC5Vc2FnZSh7XG4gICAgZGVzY3JpcHRpb246ICdDcmVhdGUgYSBuZXcgcHJvamVjdCB3aXRoIHByZS1jb25maWd1cmVkIGJvaWxlcnBsYXRlJyxcbiAgfSlcblxuICAkJHBhdGggPSBPcHRpb24uU3RyaW5nKHsgcmVxdWlyZWQ6IGZhbHNlIH0pXG5cbiAgJCRuYW1lPzogc3RyaW5nID0gT3B0aW9uLlN0cmluZygnLS1uYW1lLC1uJywge1xuICAgIGRlc2NyaXB0aW9uOlxuICAgICAgJ1RoZSBuYW1lIG9mIHRoZSBwcm9qZWN0LCBkZWZhdWx0IHRvIHRoZSBuYW1lIG9mIHRoZSBkaXJlY3RvcnkgaWYgbm90IHByb3ZpZGVkJyxcbiAgfSlcblxuICBtaW5Ob2RlQXBpVmVyc2lvbiA9IE9wdGlvbi5TdHJpbmcoJy0tbWluLW5vZGUtYXBpLC12JywgJzQnLCB7XG4gICAgdmFsaWRhdG9yOiB0eXBhbmlvbi5pc051bWJlcigpLFxuICAgIGRlc2NyaXB0aW9uOiAnVGhlIG1pbmltdW0gTm9kZS1BUEkgdmVyc2lvbiB0byBzdXBwb3J0JyxcbiAgfSlcblxuICBwYWNrYWdlTWFuYWdlciA9IE9wdGlvbi5TdHJpbmcoJy0tcGFja2FnZS1tYW5hZ2VyJywgJ3lhcm4nLCB7XG4gICAgZGVzY3JpcHRpb246ICdUaGUgcGFja2FnZSBtYW5hZ2VyIHRvIHVzZS4gT25seSBzdXBwb3J0IHlhcm4gNC54IGZvciBub3cuJyxcbiAgfSlcblxuICBsaWNlbnNlID0gT3B0aW9uLlN0cmluZygnLS1saWNlbnNlLC1sJywgJ01JVCcsIHtcbiAgICBkZXNjcmlwdGlvbjogJ0xpY2Vuc2UgZm9yIG9wZW4tc291cmNlZCBwcm9qZWN0JyxcbiAgfSlcblxuICB0YXJnZXRzID0gT3B0aW9uLkFycmF5KCctLXRhcmdldHMsLXQnLCBbXSwge1xuICAgIGRlc2NyaXB0aW9uOiAnQWxsIHRhcmdldHMgdGhlIGNyYXRlIHdpbGwgYmUgY29tcGlsZWQgZm9yLicsXG4gIH0pXG5cbiAgZW5hYmxlRGVmYXVsdFRhcmdldHMgPSBPcHRpb24uQm9vbGVhbignLS1lbmFibGUtZGVmYXVsdC10YXJnZXRzJywgdHJ1ZSwge1xuICAgIGRlc2NyaXB0aW9uOiAnV2hldGhlciBlbmFibGUgZGVmYXVsdCB0YXJnZXRzJyxcbiAgfSlcblxuICBlbmFibGVBbGxUYXJnZXRzID0gT3B0aW9uLkJvb2xlYW4oJy0tZW5hYmxlLWFsbC10YXJnZXRzJywgZmFsc2UsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1doZXRoZXIgZW5hYmxlIGFsbCB0YXJnZXRzJyxcbiAgfSlcblxuICBlbmFibGVUeXBlRGVmID0gT3B0aW9uLkJvb2xlYW4oJy0tZW5hYmxlLXR5cGUtZGVmJywgdHJ1ZSwge1xuICAgIGRlc2NyaXB0aW9uOlxuICAgICAgJ1doZXRoZXIgZW5hYmxlIHRoZSBgdHlwZS1kZWZgIGZlYXR1cmUgZm9yIHR5cGVzY3JpcHQgZGVmaW5pdGlvbnMgYXV0by1nZW5lcmF0aW9uJyxcbiAgfSlcblxuICBlbmFibGVHaXRodWJBY3Rpb25zID0gT3B0aW9uLkJvb2xlYW4oJy0tZW5hYmxlLWdpdGh1Yi1hY3Rpb25zJywgdHJ1ZSwge1xuICAgIGRlc2NyaXB0aW9uOiAnV2hldGhlciBnZW5lcmF0ZSBwcmVjb25maWd1cmVkIEdpdEh1YiBBY3Rpb25zIHdvcmtmbG93JyxcbiAgfSlcblxuICB0ZXN0RnJhbWV3b3JrID0gT3B0aW9uLlN0cmluZygnLS10ZXN0LWZyYW1ld29yaycsICdhdmEnLCB7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnVGhlIEphdmFTY3JpcHQgdGVzdCBmcmFtZXdvcmsgdG8gdXNlLCBvbmx5IHN1cHBvcnQgYGF2YWAgZm9yIG5vdycsXG4gIH0pXG5cbiAgZHJ5UnVuID0gT3B0aW9uLkJvb2xlYW4oJy0tZHJ5LXJ1bicsIGZhbHNlLCB7XG4gICAgZGVzY3JpcHRpb246ICdXaGV0aGVyIHRvIHJ1biB0aGUgY29tbWFuZCBpbiBkcnktcnVuIG1vZGUnLFxuICB9KVxuXG4gIGdldE9wdGlvbnMoKSB7XG4gICAgcmV0dXJuIHtcbiAgICAgIHBhdGg6IHRoaXMuJCRwYXRoLFxuICAgICAgbmFtZTogdGhpcy4kJG5hbWUsXG4gICAgICBtaW5Ob2RlQXBpVmVyc2lvbjogdGhpcy5taW5Ob2RlQXBpVmVyc2lvbixcbiAgICAgIHBhY2thZ2VNYW5hZ2VyOiB0aGlzLnBhY2thZ2VNYW5hZ2VyLFxuICAgICAgbGljZW5zZTogdGhpcy5saWNlbnNlLFxuICAgICAgdGFyZ2V0czogdGhpcy50YXJnZXRzLFxuICAgICAgZW5hYmxlRGVmYXVsdFRhcmdldHM6IHRoaXMuZW5hYmxlRGVmYXVsdFRhcmdldHMsXG4gICAgICBlbmFibGVBbGxUYXJnZXRzOiB0aGlzLmVuYWJsZUFsbFRhcmdldHMsXG4gICAgICBlbmFibGVUeXBlRGVmOiB0aGlzLmVuYWJsZVR5cGVEZWYsXG4gICAgICBlbmFibGVHaXRodWJBY3Rpb25zOiB0aGlzLmVuYWJsZUdpdGh1YkFjdGlvbnMsXG4gICAgICB0ZXN0RnJhbWV3b3JrOiB0aGlzLnRlc3RGcmFtZXdvcmssXG4gICAgICBkcnlSdW46IHRoaXMuZHJ5UnVuLFxuICAgIH1cbiAgfVxufVxuXG4vKipcbiAqIENyZWF0ZSBhIG5ldyBwcm9qZWN0IHdpdGggcHJlLWNvbmZpZ3VyZWQgYm9pbGVycGxhdGVcbiAqL1xuZXhwb3J0IGludGVyZmFjZSBOZXdPcHRpb25zIHtcbiAgLyoqXG4gICAqIFRoZSBwYXRoIHdoZXJlIHRoZSBOQVBJLVJTIHByb2plY3Qgd2lsbCBiZSBjcmVhdGVkLlxuICAgKi9cbiAgcGF0aD86IHN0cmluZ1xuICAvKipcbiAgICogVGhlIG5hbWUgb2YgdGhlIHByb2plY3QsIGRlZmF1bHQgdG8gdGhlIG5hbWUgb2YgdGhlIGRpcmVjdG9yeSBpZiBub3QgcHJvdmlkZWRcbiAgICovXG4gIG5hbWU/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFRoZSBtaW5pbXVtIE5vZGUtQVBJIHZlcnNpb24gdG8gc3VwcG9ydFxuICAgKlxuICAgKiBAZGVmYXVsdCA0XG4gICAqL1xuICBtaW5Ob2RlQXBpVmVyc2lvbj86IG51bWJlclxuICAvKipcbiAgICogVGhlIHBhY2thZ2UgbWFuYWdlciB0byB1c2UuIE9ubHkgc3VwcG9ydCB5YXJuIDQueCBmb3Igbm93LlxuICAgKlxuICAgKiBAZGVmYXVsdCAneWFybidcbiAgICovXG4gIHBhY2thZ2VNYW5hZ2VyPzogc3RyaW5nXG4gIC8qKlxuICAgKiBMaWNlbnNlIGZvciBvcGVuLXNvdXJjZWQgcHJvamVjdFxuICAgKlxuICAgKiBAZGVmYXVsdCAnTUlUJ1xuICAgKi9cbiAgbGljZW5zZT86IHN0cmluZ1xuICAvKipcbiAgICogQWxsIHRhcmdldHMgdGhlIGNyYXRlIHdpbGwgYmUgY29tcGlsZWQgZm9yLlxuICAgKlxuICAgKiBAZGVmYXVsdCBbXVxuICAgKi9cbiAgdGFyZ2V0cz86IHN0cmluZ1tdXG4gIC8qKlxuICAgKiBXaGV0aGVyIGVuYWJsZSBkZWZhdWx0IHRhcmdldHNcbiAgICpcbiAgICogQGRlZmF1bHQgdHJ1ZVxuICAgKi9cbiAgZW5hYmxlRGVmYXVsdFRhcmdldHM/OiBib29sZWFuXG4gIC8qKlxuICAgKiBXaGV0aGVyIGVuYWJsZSBhbGwgdGFyZ2V0c1xuICAgKlxuICAgKiBAZGVmYXVsdCBmYWxzZVxuICAgKi9cbiAgZW5hYmxlQWxsVGFyZ2V0cz86IGJvb2xlYW5cbiAgLyoqXG4gICAqIFdoZXRoZXIgZW5hYmxlIHRoZSBgdHlwZS1kZWZgIGZlYXR1cmUgZm9yIHR5cGVzY3JpcHQgZGVmaW5pdGlvbnMgYXV0by1nZW5lcmF0aW9uXG4gICAqXG4gICAqIEBkZWZhdWx0IHRydWVcbiAgICovXG4gIGVuYWJsZVR5cGVEZWY/OiBib29sZWFuXG4gIC8qKlxuICAgKiBXaGV0aGVyIGdlbmVyYXRlIHByZWNvbmZpZ3VyZWQgR2l0SHViIEFjdGlvbnMgd29ya2Zsb3dcbiAgICpcbiAgICogQGRlZmF1bHQgdHJ1ZVxuICAgKi9cbiAgZW5hYmxlR2l0aHViQWN0aW9ucz86IGJvb2xlYW5cbiAgLyoqXG4gICAqIFRoZSBKYXZhU2NyaXB0IHRlc3QgZnJhbWV3b3JrIHRvIHVzZSwgb25seSBzdXBwb3J0IGBhdmFgIGZvciBub3dcbiAgICpcbiAgICogQGRlZmF1bHQgJ2F2YSdcbiAgICovXG4gIHRlc3RGcmFtZXdvcms/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFdoZXRoZXIgdG8gcnVuIHRoZSBjb21tYW5kIGluIGRyeS1ydW4gbW9kZVxuICAgKlxuICAgKiBAZGVmYXVsdCBmYWxzZVxuICAgKi9cbiAgZHJ5UnVuPzogYm9vbGVhblxufVxuXG5leHBvcnQgZnVuY3Rpb24gYXBwbHlEZWZhdWx0TmV3T3B0aW9ucyhvcHRpb25zOiBOZXdPcHRpb25zKSB7XG4gIHJldHVybiB7XG4gICAgbWluTm9kZUFwaVZlcnNpb246IDQsXG4gICAgcGFja2FnZU1hbmFnZXI6ICd5YXJuJyxcbiAgICBsaWNlbnNlOiAnTUlUJyxcbiAgICB0YXJnZXRzOiBbXSxcbiAgICBlbmFibGVEZWZhdWx0VGFyZ2V0czogdHJ1ZSxcbiAgICBlbmFibGVBbGxUYXJnZXRzOiBmYWxzZSxcbiAgICBlbmFibGVUeXBlRGVmOiB0cnVlLFxuICAgIGVuYWJsZUdpdGh1YkFjdGlvbnM6IHRydWUsXG4gICAgdGVzdEZyYW1ld29yazogJ2F2YScsXG4gICAgZHJ5UnVuOiBmYWxzZSxcbiAgICAuLi5vcHRpb25zLFxuICB9XG59XG4iLCIvLyBDb3B5cmlnaHQgMjAxOC0yMDI1IHRoZSBEZW5vIGF1dGhvcnMuIE1JVCBsaWNlbnNlLlxuLy8gVGhpcyBtb2R1bGUgaXMgYnJvd3NlciBjb21wYXRpYmxlLlxuLy8gQmFyZSBrZXlzIG1heSBvbmx5IGNvbnRhaW4gQVNDSUkgbGV0dGVycyxcbi8vIEFTQ0lJIGRpZ2l0cywgdW5kZXJzY29yZXMsIGFuZCBkYXNoZXMgKEEtWmEtejAtOV8tKS5cbmZ1bmN0aW9uIGpvaW5LZXlzKGtleXMpIHtcbiAgLy8gRG90dGVkIGtleXMgYXJlIGEgc2VxdWVuY2Ugb2YgYmFyZSBvciBxdW90ZWQga2V5cyBqb2luZWQgd2l0aCBhIGRvdC5cbiAgLy8gVGhpcyBhbGxvd3MgZm9yIGdyb3VwaW5nIHNpbWlsYXIgcHJvcGVydGllcyB0b2dldGhlcjpcbiAgcmV0dXJuIGtleXMubWFwKChzdHIpPT57XG4gICAgcmV0dXJuIHN0ci5sZW5ndGggPT09IDAgfHwgc3RyLm1hdGNoKC9bXkEtWmEtejAtOV8tXS8pID8gSlNPTi5zdHJpbmdpZnkoc3RyKSA6IHN0cjtcbiAgfSkuam9pbihcIi5cIik7XG59XG5jbGFzcyBEdW1wZXIge1xuICBtYXhQYWQgPSAwO1xuICBzcmNPYmplY3Q7XG4gIG91dHB1dCA9IFtdO1xuICAjYXJyYXlUeXBlQ2FjaGUgPSBuZXcgTWFwKCk7XG4gIGNvbnN0cnVjdG9yKHNyY09iamMpe1xuICAgIHRoaXMuc3JjT2JqZWN0ID0gc3JjT2JqYztcbiAgfVxuICBkdW1wKGZtdE9wdGlvbnMgPSB7fSkge1xuICAgIC8vIGRlbm8tbGludC1pZ25vcmUgbm8tZXhwbGljaXQtYW55XG4gICAgdGhpcy5vdXRwdXQgPSB0aGlzLiNwcmludE9iamVjdCh0aGlzLnNyY09iamVjdCk7XG4gICAgdGhpcy5vdXRwdXQgPSB0aGlzLiNmb3JtYXQoZm10T3B0aW9ucyk7XG4gICAgcmV0dXJuIHRoaXMub3V0cHV0O1xuICB9XG4gICNwcmludE9iamVjdChvYmosIGtleXMgPSBbXSkge1xuICAgIGNvbnN0IG91dCA9IFtdO1xuICAgIGNvbnN0IHByb3BzID0gT2JqZWN0LmtleXMob2JqKTtcbiAgICBjb25zdCBpbmxpbmVQcm9wcyA9IFtdO1xuICAgIGNvbnN0IG11bHRpbGluZVByb3BzID0gW107XG4gICAgZm9yIChjb25zdCBwcm9wIG9mIHByb3BzKXtcbiAgICAgIGlmICh0aGlzLiNpc1NpbXBseVNlcmlhbGl6YWJsZShvYmpbcHJvcF0pKSB7XG4gICAgICAgIGlubGluZVByb3BzLnB1c2gocHJvcCk7XG4gICAgICB9IGVsc2Uge1xuICAgICAgICBtdWx0aWxpbmVQcm9wcy5wdXNoKHByb3ApO1xuICAgICAgfVxuICAgIH1cbiAgICBjb25zdCBzb3J0ZWRQcm9wcyA9IGlubGluZVByb3BzLmNvbmNhdChtdWx0aWxpbmVQcm9wcyk7XG4gICAgZm9yIChjb25zdCBwcm9wIG9mIHNvcnRlZFByb3BzKXtcbiAgICAgIGNvbnN0IHZhbHVlID0gb2JqW3Byb3BdO1xuICAgICAgaWYgKHZhbHVlIGluc3RhbmNlb2YgRGF0ZSkge1xuICAgICAgICBvdXQucHVzaCh0aGlzLiNkYXRlRGVjbGFyYXRpb24oW1xuICAgICAgICAgIHByb3BcbiAgICAgICAgXSwgdmFsdWUpKTtcbiAgICAgIH0gZWxzZSBpZiAodHlwZW9mIHZhbHVlID09PSBcInN0cmluZ1wiIHx8IHZhbHVlIGluc3RhbmNlb2YgUmVnRXhwKSB7XG4gICAgICAgIG91dC5wdXNoKHRoaXMuI3N0ckRlY2xhcmF0aW9uKFtcbiAgICAgICAgICBwcm9wXG4gICAgICAgIF0sIHZhbHVlLnRvU3RyaW5nKCkpKTtcbiAgICAgIH0gZWxzZSBpZiAodHlwZW9mIHZhbHVlID09PSBcIm51bWJlclwiKSB7XG4gICAgICAgIG91dC5wdXNoKHRoaXMuI251bWJlckRlY2xhcmF0aW9uKFtcbiAgICAgICAgICBwcm9wXG4gICAgICAgIF0sIHZhbHVlKSk7XG4gICAgICB9IGVsc2UgaWYgKHR5cGVvZiB2YWx1ZSA9PT0gXCJib29sZWFuXCIpIHtcbiAgICAgICAgb3V0LnB1c2godGhpcy4jYm9vbERlY2xhcmF0aW9uKFtcbiAgICAgICAgICBwcm9wXG4gICAgICAgIF0sIHZhbHVlKSk7XG4gICAgICB9IGVsc2UgaWYgKHZhbHVlIGluc3RhbmNlb2YgQXJyYXkpIHtcbiAgICAgICAgY29uc3QgYXJyYXlUeXBlID0gdGhpcy4jZ2V0VHlwZU9mQXJyYXkodmFsdWUpO1xuICAgICAgICBpZiAoYXJyYXlUeXBlID09PSBcIk9OTFlfUFJJTUlUSVZFXCIpIHtcbiAgICAgICAgICBvdXQucHVzaCh0aGlzLiNhcnJheURlY2xhcmF0aW9uKFtcbiAgICAgICAgICAgIHByb3BcbiAgICAgICAgICBdLCB2YWx1ZSkpO1xuICAgICAgICB9IGVsc2UgaWYgKGFycmF5VHlwZSA9PT0gXCJPTkxZX09CSkVDVF9FWENMVURJTkdfQVJSQVlcIikge1xuICAgICAgICAgIC8vIGFycmF5IG9mIG9iamVjdHNcbiAgICAgICAgICBmb3IobGV0IGkgPSAwOyBpIDwgdmFsdWUubGVuZ3RoOyBpKyspe1xuICAgICAgICAgICAgb3V0LnB1c2goXCJcIik7XG4gICAgICAgICAgICBvdXQucHVzaCh0aGlzLiNoZWFkZXJHcm91cChbXG4gICAgICAgICAgICAgIC4uLmtleXMsXG4gICAgICAgICAgICAgIHByb3BcbiAgICAgICAgICAgIF0pKTtcbiAgICAgICAgICAgIG91dC5wdXNoKC4uLnRoaXMuI3ByaW50T2JqZWN0KHZhbHVlW2ldLCBbXG4gICAgICAgICAgICAgIC4uLmtleXMsXG4gICAgICAgICAgICAgIHByb3BcbiAgICAgICAgICAgIF0pKTtcbiAgICAgICAgICB9XG4gICAgICAgIH0gZWxzZSB7XG4gICAgICAgICAgLy8gdGhpcyBpcyBhIGNvbXBsZXggYXJyYXksIHVzZSB0aGUgaW5saW5lIGZvcm1hdC5cbiAgICAgICAgICBjb25zdCBzdHIgPSB2YWx1ZS5tYXAoKHgpPT50aGlzLiNwcmludEFzSW5saW5lVmFsdWUoeCkpLmpvaW4oXCIsXCIpO1xuICAgICAgICAgIG91dC5wdXNoKGAke3RoaXMuI2RlY2xhcmF0aW9uKFtcbiAgICAgICAgICAgIHByb3BcbiAgICAgICAgICBdKX1bJHtzdHJ9XWApO1xuICAgICAgICB9XG4gICAgICB9IGVsc2UgaWYgKHR5cGVvZiB2YWx1ZSA9PT0gXCJvYmplY3RcIikge1xuICAgICAgICBvdXQucHVzaChcIlwiKTtcbiAgICAgICAgb3V0LnB1c2godGhpcy4jaGVhZGVyKFtcbiAgICAgICAgICAuLi5rZXlzLFxuICAgICAgICAgIHByb3BcbiAgICAgICAgXSkpO1xuICAgICAgICBpZiAodmFsdWUpIHtcbiAgICAgICAgICBjb25zdCB0b1BhcnNlID0gdmFsdWU7XG4gICAgICAgICAgb3V0LnB1c2goLi4udGhpcy4jcHJpbnRPYmplY3QodG9QYXJzZSwgW1xuICAgICAgICAgICAgLi4ua2V5cyxcbiAgICAgICAgICAgIHByb3BcbiAgICAgICAgICBdKSk7XG4gICAgICAgIH1cbiAgICAgIC8vIG91dC5wdXNoKC4uLnRoaXMuX3BhcnNlKHZhbHVlLCBgJHtwYXRofSR7cHJvcH0uYCkpO1xuICAgICAgfVxuICAgIH1cbiAgICBvdXQucHVzaChcIlwiKTtcbiAgICByZXR1cm4gb3V0O1xuICB9XG4gICNpc1ByaW1pdGl2ZSh2YWx1ZSkge1xuICAgIHJldHVybiB2YWx1ZSBpbnN0YW5jZW9mIERhdGUgfHwgdmFsdWUgaW5zdGFuY2VvZiBSZWdFeHAgfHwgW1xuICAgICAgXCJzdHJpbmdcIixcbiAgICAgIFwibnVtYmVyXCIsXG4gICAgICBcImJvb2xlYW5cIlxuICAgIF0uaW5jbHVkZXModHlwZW9mIHZhbHVlKTtcbiAgfVxuICAjZ2V0VHlwZU9mQXJyYXkoYXJyKSB7XG4gICAgaWYgKHRoaXMuI2FycmF5VHlwZUNhY2hlLmhhcyhhcnIpKSB7XG4gICAgICByZXR1cm4gdGhpcy4jYXJyYXlUeXBlQ2FjaGUuZ2V0KGFycik7XG4gICAgfVxuICAgIGNvbnN0IHR5cGUgPSB0aGlzLiNkb0dldFR5cGVPZkFycmF5KGFycik7XG4gICAgdGhpcy4jYXJyYXlUeXBlQ2FjaGUuc2V0KGFyciwgdHlwZSk7XG4gICAgcmV0dXJuIHR5cGU7XG4gIH1cbiAgI2RvR2V0VHlwZU9mQXJyYXkoYXJyKSB7XG4gICAgaWYgKCFhcnIubGVuZ3RoKSB7XG4gICAgICAvLyBhbnkgdHlwZSBzaG91bGQgYmUgZmluZVxuICAgICAgcmV0dXJuIFwiT05MWV9QUklNSVRJVkVcIjtcbiAgICB9XG4gICAgY29uc3Qgb25seVByaW1pdGl2ZSA9IHRoaXMuI2lzUHJpbWl0aXZlKGFyclswXSk7XG4gICAgaWYgKGFyclswXSBpbnN0YW5jZW9mIEFycmF5KSB7XG4gICAgICByZXR1cm4gXCJNSVhFRFwiO1xuICAgIH1cbiAgICBmb3IobGV0IGkgPSAxOyBpIDwgYXJyLmxlbmd0aDsgaSsrKXtcbiAgICAgIGlmIChvbmx5UHJpbWl0aXZlICE9PSB0aGlzLiNpc1ByaW1pdGl2ZShhcnJbaV0pIHx8IGFycltpXSBpbnN0YW5jZW9mIEFycmF5KSB7XG4gICAgICAgIHJldHVybiBcIk1JWEVEXCI7XG4gICAgICB9XG4gICAgfVxuICAgIHJldHVybiBvbmx5UHJpbWl0aXZlID8gXCJPTkxZX1BSSU1JVElWRVwiIDogXCJPTkxZX09CSkVDVF9FWENMVURJTkdfQVJSQVlcIjtcbiAgfVxuICAjcHJpbnRBc0lubGluZVZhbHVlKHZhbHVlKSB7XG4gICAgaWYgKHZhbHVlIGluc3RhbmNlb2YgRGF0ZSkge1xuICAgICAgcmV0dXJuIGBcIiR7dGhpcy4jcHJpbnREYXRlKHZhbHVlKX1cImA7XG4gICAgfSBlbHNlIGlmICh0eXBlb2YgdmFsdWUgPT09IFwic3RyaW5nXCIgfHwgdmFsdWUgaW5zdGFuY2VvZiBSZWdFeHApIHtcbiAgICAgIHJldHVybiBKU09OLnN0cmluZ2lmeSh2YWx1ZS50b1N0cmluZygpKTtcbiAgICB9IGVsc2UgaWYgKHR5cGVvZiB2YWx1ZSA9PT0gXCJudW1iZXJcIikge1xuICAgICAgcmV0dXJuIHZhbHVlO1xuICAgIH0gZWxzZSBpZiAodHlwZW9mIHZhbHVlID09PSBcImJvb2xlYW5cIikge1xuICAgICAgcmV0dXJuIHZhbHVlLnRvU3RyaW5nKCk7XG4gICAgfSBlbHNlIGlmICh2YWx1ZSBpbnN0YW5jZW9mIEFycmF5KSB7XG4gICAgICBjb25zdCBzdHIgPSB2YWx1ZS5tYXAoKHgpPT50aGlzLiNwcmludEFzSW5saW5lVmFsdWUoeCkpLmpvaW4oXCIsXCIpO1xuICAgICAgcmV0dXJuIGBbJHtzdHJ9XWA7XG4gICAgfSBlbHNlIGlmICh0eXBlb2YgdmFsdWUgPT09IFwib2JqZWN0XCIpIHtcbiAgICAgIGlmICghdmFsdWUpIHtcbiAgICAgICAgdGhyb3cgbmV3IEVycm9yKFwiU2hvdWxkIG5ldmVyIHJlYWNoXCIpO1xuICAgICAgfVxuICAgICAgY29uc3Qgc3RyID0gT2JqZWN0LmtleXModmFsdWUpLm1hcCgoa2V5KT0+e1xuICAgICAgICByZXR1cm4gYCR7am9pbktleXMoW1xuICAgICAgICAgIGtleVxuICAgICAgICBdKX0gPSAkey8vIGRlbm8tbGludC1pZ25vcmUgbm8tZXhwbGljaXQtYW55XG4gICAgICAgIHRoaXMuI3ByaW50QXNJbmxpbmVWYWx1ZSh2YWx1ZVtrZXldKX1gO1xuICAgICAgfSkuam9pbihcIixcIik7XG4gICAgICByZXR1cm4gYHske3N0cn19YDtcbiAgICB9XG4gICAgdGhyb3cgbmV3IEVycm9yKFwiU2hvdWxkIG5ldmVyIHJlYWNoXCIpO1xuICB9XG4gICNpc1NpbXBseVNlcmlhbGl6YWJsZSh2YWx1ZSkge1xuICAgIHJldHVybiB0eXBlb2YgdmFsdWUgPT09IFwic3RyaW5nXCIgfHwgdHlwZW9mIHZhbHVlID09PSBcIm51bWJlclwiIHx8IHR5cGVvZiB2YWx1ZSA9PT0gXCJib29sZWFuXCIgfHwgdmFsdWUgaW5zdGFuY2VvZiBSZWdFeHAgfHwgdmFsdWUgaW5zdGFuY2VvZiBEYXRlIHx8IHZhbHVlIGluc3RhbmNlb2YgQXJyYXkgJiYgdGhpcy4jZ2V0VHlwZU9mQXJyYXkodmFsdWUpICE9PSBcIk9OTFlfT0JKRUNUX0VYQ0xVRElOR19BUlJBWVwiO1xuICB9XG4gICNoZWFkZXIoa2V5cykge1xuICAgIHJldHVybiBgWyR7am9pbktleXMoa2V5cyl9XWA7XG4gIH1cbiAgI2hlYWRlckdyb3VwKGtleXMpIHtcbiAgICByZXR1cm4gYFtbJHtqb2luS2V5cyhrZXlzKX1dXWA7XG4gIH1cbiAgI2RlY2xhcmF0aW9uKGtleXMpIHtcbiAgICBjb25zdCB0aXRsZSA9IGpvaW5LZXlzKGtleXMpO1xuICAgIGlmICh0aXRsZS5sZW5ndGggPiB0aGlzLm1heFBhZCkge1xuICAgICAgdGhpcy5tYXhQYWQgPSB0aXRsZS5sZW5ndGg7XG4gICAgfVxuICAgIHJldHVybiBgJHt0aXRsZX0gPSBgO1xuICB9XG4gICNhcnJheURlY2xhcmF0aW9uKGtleXMsIHZhbHVlKSB7XG4gICAgcmV0dXJuIGAke3RoaXMuI2RlY2xhcmF0aW9uKGtleXMpfSR7SlNPTi5zdHJpbmdpZnkodmFsdWUpfWA7XG4gIH1cbiAgI3N0ckRlY2xhcmF0aW9uKGtleXMsIHZhbHVlKSB7XG4gICAgcmV0dXJuIGAke3RoaXMuI2RlY2xhcmF0aW9uKGtleXMpfSR7SlNPTi5zdHJpbmdpZnkodmFsdWUpfWA7XG4gIH1cbiAgI251bWJlckRlY2xhcmF0aW9uKGtleXMsIHZhbHVlKSB7XG4gICAgaWYgKE51bWJlci5pc05hTih2YWx1ZSkpIHtcbiAgICAgIHJldHVybiBgJHt0aGlzLiNkZWNsYXJhdGlvbihrZXlzKX1uYW5gO1xuICAgIH1cbiAgICBzd2l0Y2godmFsdWUpe1xuICAgICAgY2FzZSBJbmZpbml0eTpcbiAgICAgICAgcmV0dXJuIGAke3RoaXMuI2RlY2xhcmF0aW9uKGtleXMpfWluZmA7XG4gICAgICBjYXNlIC1JbmZpbml0eTpcbiAgICAgICAgcmV0dXJuIGAke3RoaXMuI2RlY2xhcmF0aW9uKGtleXMpfS1pbmZgO1xuICAgICAgZGVmYXVsdDpcbiAgICAgICAgcmV0dXJuIGAke3RoaXMuI2RlY2xhcmF0aW9uKGtleXMpfSR7dmFsdWV9YDtcbiAgICB9XG4gIH1cbiAgI2Jvb2xEZWNsYXJhdGlvbihrZXlzLCB2YWx1ZSkge1xuICAgIHJldHVybiBgJHt0aGlzLiNkZWNsYXJhdGlvbihrZXlzKX0ke3ZhbHVlfWA7XG4gIH1cbiAgI3ByaW50RGF0ZSh2YWx1ZSkge1xuICAgIGZ1bmN0aW9uIGR0UGFkKHYsIGxQYWQgPSAyKSB7XG4gICAgICByZXR1cm4gdi5wYWRTdGFydChsUGFkLCBcIjBcIik7XG4gICAgfVxuICAgIGNvbnN0IG0gPSBkdFBhZCgodmFsdWUuZ2V0VVRDTW9udGgoKSArIDEpLnRvU3RyaW5nKCkpO1xuICAgIGNvbnN0IGQgPSBkdFBhZCh2YWx1ZS5nZXRVVENEYXRlKCkudG9TdHJpbmcoKSk7XG4gICAgY29uc3QgaCA9IGR0UGFkKHZhbHVlLmdldFVUQ0hvdXJzKCkudG9TdHJpbmcoKSk7XG4gICAgY29uc3QgbWluID0gZHRQYWQodmFsdWUuZ2V0VVRDTWludXRlcygpLnRvU3RyaW5nKCkpO1xuICAgIGNvbnN0IHMgPSBkdFBhZCh2YWx1ZS5nZXRVVENTZWNvbmRzKCkudG9TdHJpbmcoKSk7XG4gICAgY29uc3QgbXMgPSBkdFBhZCh2YWx1ZS5nZXRVVENNaWxsaXNlY29uZHMoKS50b1N0cmluZygpLCAzKTtcbiAgICAvLyBmb3JtYXR0ZWQgZGF0ZVxuICAgIGNvbnN0IGZEYXRhID0gYCR7dmFsdWUuZ2V0VVRDRnVsbFllYXIoKX0tJHttfS0ke2R9VCR7aH06JHttaW59OiR7c30uJHttc31gO1xuICAgIHJldHVybiBmRGF0YTtcbiAgfVxuICAjZGF0ZURlY2xhcmF0aW9uKGtleXMsIHZhbHVlKSB7XG4gICAgcmV0dXJuIGAke3RoaXMuI2RlY2xhcmF0aW9uKGtleXMpfSR7dGhpcy4jcHJpbnREYXRlKHZhbHVlKX1gO1xuICB9XG4gICNmb3JtYXQob3B0aW9ucyA9IHt9KSB7XG4gICAgY29uc3QgeyBrZXlBbGlnbm1lbnQgPSBmYWxzZSB9ID0gb3B0aW9ucztcbiAgICBjb25zdCByRGVjbGFyYXRpb24gPSAvXihcXFwiLipcXFwifFtePV0qKVxccz0vO1xuICAgIGNvbnN0IG91dCA9IFtdO1xuICAgIGZvcihsZXQgaSA9IDA7IGkgPCB0aGlzLm91dHB1dC5sZW5ndGg7IGkrKyl7XG4gICAgICBjb25zdCBsID0gdGhpcy5vdXRwdXRbaV07XG4gICAgICAvLyB3ZSBrZWVwIGVtcHR5IGVudHJ5IGZvciBhcnJheSBvZiBvYmplY3RzXG4gICAgICBpZiAobFswXSA9PT0gXCJbXCIgJiYgbFsxXSAhPT0gXCJbXCIpIHtcbiAgICAgICAgLy8gbm9uLWVtcHR5IG9iamVjdCB3aXRoIG9ubHkgc3Vib2JqZWN0cyBhcyBwcm9wZXJ0aWVzXG4gICAgICAgIGlmICh0aGlzLm91dHB1dFtpICsgMV0gPT09IFwiXCIgJiYgdGhpcy5vdXRwdXRbaSArIDJdPy5zbGljZSgwLCBsLmxlbmd0aCkgPT09IGwuc2xpY2UoMCwgLTEpICsgXCIuXCIpIHtcbiAgICAgICAgICBpICs9IDE7XG4gICAgICAgICAgY29udGludWU7XG4gICAgICAgIH1cbiAgICAgICAgb3V0LnB1c2gobCk7XG4gICAgICB9IGVsc2Uge1xuICAgICAgICBpZiAoa2V5QWxpZ25tZW50KSB7XG4gICAgICAgICAgY29uc3QgbSA9IHJEZWNsYXJhdGlvbi5leGVjKGwpO1xuICAgICAgICAgIGlmIChtICYmIG1bMV0pIHtcbiAgICAgICAgICAgIG91dC5wdXNoKGwucmVwbGFjZShtWzFdLCBtWzFdLnBhZEVuZCh0aGlzLm1heFBhZCkpKTtcbiAgICAgICAgICB9IGVsc2Uge1xuICAgICAgICAgICAgb3V0LnB1c2gobCk7XG4gICAgICAgICAgfVxuICAgICAgICB9IGVsc2Uge1xuICAgICAgICAgIG91dC5wdXNoKGwpO1xuICAgICAgICB9XG4gICAgICB9XG4gICAgfVxuICAgIC8vIENsZWFuaW5nIG11bHRpcGxlIHNwYWNlc1xuICAgIGNvbnN0IGNsZWFuZWRPdXRwdXQgPSBbXTtcbiAgICBmb3IobGV0IGkgPSAwOyBpIDwgb3V0Lmxlbmd0aDsgaSsrKXtcbiAgICAgIGNvbnN0IGwgPSBvdXRbaV07XG4gICAgICBpZiAoIShsID09PSBcIlwiICYmIG91dFtpICsgMV0gPT09IFwiXCIpKSB7XG4gICAgICAgIGNsZWFuZWRPdXRwdXQucHVzaChsKTtcbiAgICAgIH1cbiAgICB9XG4gICAgcmV0dXJuIGNsZWFuZWRPdXRwdXQ7XG4gIH1cbn1cbi8qKlxuICogQ29udmVydHMgYW4gb2JqZWN0IHRvIGEge0BsaW5rIGh0dHBzOi8vdG9tbC5pbyB8IFRPTUx9IHN0cmluZy5cbiAqXG4gKiBAZXhhbXBsZSBVc2FnZVxuICogYGBgdHNcbiAqIGltcG9ydCB7IHN0cmluZ2lmeSB9IGZyb20gXCJAc3RkL3RvbWwvc3RyaW5naWZ5XCI7XG4gKiBpbXBvcnQgeyBhc3NlcnRFcXVhbHMgfSBmcm9tIFwiQHN0ZC9hc3NlcnRcIjtcbiAqXG4gKiBjb25zdCBvYmogPSB7XG4gKiAgIHRpdGxlOiBcIlRPTUwgRXhhbXBsZVwiLFxuICogICBvd25lcjoge1xuICogICAgIG5hbWU6IFwiQm9iXCIsXG4gKiAgICAgYmlvOiBcIkJvYiBpcyBhIGNvb2wgZ3V5XCIsXG4gKiAgfVxuICogfTtcbiAqIGNvbnN0IHRvbWxTdHJpbmcgPSBzdHJpbmdpZnkob2JqKTtcbiAqIGFzc2VydEVxdWFscyh0b21sU3RyaW5nLCBgdGl0bGUgPSBcIlRPTUwgRXhhbXBsZVwiXFxuXFxuW293bmVyXVxcbm5hbWUgPSBcIkJvYlwiXFxuYmlvID0gXCJCb2IgaXMgYSBjb29sIGd1eVwiXFxuYCk7XG4gKiBgYGBcbiAqIEBwYXJhbSBvYmogU291cmNlIG9iamVjdFxuICogQHBhcmFtIG9wdGlvbnMgT3B0aW9ucyBmb3Igc3RyaW5naWZ5aW5nLlxuICogQHJldHVybnMgVE9NTCBzdHJpbmdcbiAqLyBleHBvcnQgZnVuY3Rpb24gc3RyaW5naWZ5KG9iaiwgb3B0aW9ucykge1xuICByZXR1cm4gbmV3IER1bXBlcihvYmopLmR1bXAob3B0aW9ucykuam9pbihcIlxcblwiKTtcbn1cbi8vIyBzb3VyY2VNYXBwaW5nVVJMPXN0cmluZ2lmeS5qcy5tYXAiLCIvLyBDb3B5cmlnaHQgMjAxOC0yMDI1IHRoZSBEZW5vIGF1dGhvcnMuIE1JVCBsaWNlbnNlLlxuLy8gVGhpcyBtb2R1bGUgaXMgYnJvd3NlciBjb21wYXRpYmxlLlxuLyoqXG4gKiBGaWx0ZXJzIHRoZSBnaXZlbiBhcnJheSwgcmVtb3ZpbmcgYWxsIGVsZW1lbnRzIHRoYXQgZG8gbm90IG1hdGNoIHRoZSBnaXZlbiBwcmVkaWNhdGVcbiAqICoqaW4gcGxhY2UuIFRoaXMgbWVhbnMgYGFycmF5YCB3aWxsIGJlIG1vZGlmaWVkISoqLlxuICovIGV4cG9ydCBmdW5jdGlvbiBmaWx0ZXJJblBsYWNlKGFycmF5LCBwcmVkaWNhdGUpIHtcbiAgbGV0IG91dHB1dEluZGV4ID0gMDtcbiAgZm9yIChjb25zdCBjdXIgb2YgYXJyYXkpe1xuICAgIGlmICghcHJlZGljYXRlKGN1cikpIHtcbiAgICAgIGNvbnRpbnVlO1xuICAgIH1cbiAgICBhcnJheVtvdXRwdXRJbmRleF0gPSBjdXI7XG4gICAgb3V0cHV0SW5kZXggKz0gMTtcbiAgfVxuICBhcnJheS5zcGxpY2Uob3V0cHV0SW5kZXgpO1xuICByZXR1cm4gYXJyYXk7XG59XG4vLyMgc291cmNlTWFwcGluZ1VSTD1fdXRpbHMuanMubWFwIiwiLy8gQ29weXJpZ2h0IDIwMTgtMjAyNSB0aGUgRGVubyBhdXRob3JzLiBNSVQgbGljZW5zZS5cbi8vIFRoaXMgbW9kdWxlIGlzIGJyb3dzZXIgY29tcGF0aWJsZS5cbmltcG9ydCB7IGZpbHRlckluUGxhY2UgfSBmcm9tIFwiLi9fdXRpbHMuanNcIjtcbmV4cG9ydCBmdW5jdGlvbiBkZWVwTWVyZ2UocmVjb3JkLCBvdGhlciwgb3B0aW9ucykge1xuICByZXR1cm4gZGVlcE1lcmdlSW50ZXJuYWwocmVjb3JkLCBvdGhlciwgbmV3IFNldCgpLCBvcHRpb25zKTtcbn1cbmZ1bmN0aW9uIGRlZXBNZXJnZUludGVybmFsKHJlY29yZCwgb3RoZXIsIHNlZW4sIG9wdGlvbnMpIHtcbiAgY29uc3QgcmVzdWx0ID0ge307XG4gIGNvbnN0IGtleXMgPSBuZXcgU2V0KFtcbiAgICAuLi5nZXRLZXlzKHJlY29yZCksXG4gICAgLi4uZ2V0S2V5cyhvdGhlcilcbiAgXSk7XG4gIC8vIEl0ZXJhdGUgdGhyb3VnaCBlYWNoIGtleSBvZiBvdGhlciBvYmplY3QgYW5kIHVzZSBjb3JyZWN0IG1lcmdpbmcgc3RyYXRlZ3lcbiAgZm9yIChjb25zdCBrZXkgb2Yga2V5cyl7XG4gICAgLy8gU2tpcCB0byBwcmV2ZW50IE9iamVjdC5wcm90b3R5cGUuX19wcm90b19fIGFjY2Vzc29yIHByb3BlcnR5IGNhbGxzIG9uIG5vbi1EZW5vIHBsYXRmb3Jtc1xuICAgIGlmIChrZXkgPT09IFwiX19wcm90b19fXCIpIHtcbiAgICAgIGNvbnRpbnVlO1xuICAgIH1cbiAgICBjb25zdCBhID0gcmVjb3JkW2tleV07XG4gICAgaWYgKCFPYmplY3QuaGFzT3duKG90aGVyLCBrZXkpKSB7XG4gICAgICByZXN1bHRba2V5XSA9IGE7XG4gICAgICBjb250aW51ZTtcbiAgICB9XG4gICAgY29uc3QgYiA9IG90aGVyW2tleV07XG4gICAgaWYgKGlzTm9uTnVsbE9iamVjdChhKSAmJiBpc05vbk51bGxPYmplY3QoYikgJiYgIXNlZW4uaGFzKGEpICYmICFzZWVuLmhhcyhiKSkge1xuICAgICAgc2Vlbi5hZGQoYSk7XG4gICAgICBzZWVuLmFkZChiKTtcbiAgICAgIHJlc3VsdFtrZXldID0gbWVyZ2VPYmplY3RzKGEsIGIsIHNlZW4sIG9wdGlvbnMpO1xuICAgICAgY29udGludWU7XG4gICAgfVxuICAgIC8vIE92ZXJyaWRlIHZhbHVlXG4gICAgcmVzdWx0W2tleV0gPSBiO1xuICB9XG4gIHJldHVybiByZXN1bHQ7XG59XG5mdW5jdGlvbiBtZXJnZU9iamVjdHMobGVmdCwgcmlnaHQsIHNlZW4sIG9wdGlvbnMgPSB7XG4gIGFycmF5czogXCJtZXJnZVwiLFxuICBzZXRzOiBcIm1lcmdlXCIsXG4gIG1hcHM6IFwibWVyZ2VcIlxufSkge1xuICAvLyBSZWN1cnNpdmVseSBtZXJnZSBtZXJnZWFibGUgb2JqZWN0c1xuICBpZiAoaXNNZXJnZWFibGUobGVmdCkgJiYgaXNNZXJnZWFibGUocmlnaHQpKSB7XG4gICAgcmV0dXJuIGRlZXBNZXJnZUludGVybmFsKGxlZnQsIHJpZ2h0LCBzZWVuLCBvcHRpb25zKTtcbiAgfVxuICBpZiAoaXNJdGVyYWJsZShsZWZ0KSAmJiBpc0l0ZXJhYmxlKHJpZ2h0KSkge1xuICAgIC8vIEhhbmRsZSBhcnJheXNcbiAgICBpZiAoQXJyYXkuaXNBcnJheShsZWZ0KSAmJiBBcnJheS5pc0FycmF5KHJpZ2h0KSkge1xuICAgICAgaWYgKG9wdGlvbnMuYXJyYXlzID09PSBcIm1lcmdlXCIpIHtcbiAgICAgICAgcmV0dXJuIGxlZnQuY29uY2F0KHJpZ2h0KTtcbiAgICAgIH1cbiAgICAgIHJldHVybiByaWdodDtcbiAgICB9XG4gICAgLy8gSGFuZGxlIG1hcHNcbiAgICBpZiAobGVmdCBpbnN0YW5jZW9mIE1hcCAmJiByaWdodCBpbnN0YW5jZW9mIE1hcCkge1xuICAgICAgaWYgKG9wdGlvbnMubWFwcyA9PT0gXCJtZXJnZVwiKSB7XG4gICAgICAgIHJldHVybiBuZXcgTWFwKFtcbiAgICAgICAgICAuLi5sZWZ0LFxuICAgICAgICAgIC4uLnJpZ2h0XG4gICAgICAgIF0pO1xuICAgICAgfVxuICAgICAgcmV0dXJuIHJpZ2h0O1xuICAgIH1cbiAgICAvLyBIYW5kbGUgc2V0c1xuICAgIGlmIChsZWZ0IGluc3RhbmNlb2YgU2V0ICYmIHJpZ2h0IGluc3RhbmNlb2YgU2V0KSB7XG4gICAgICBpZiAob3B0aW9ucy5zZXRzID09PSBcIm1lcmdlXCIpIHtcbiAgICAgICAgcmV0dXJuIG5ldyBTZXQoW1xuICAgICAgICAgIC4uLmxlZnQsXG4gICAgICAgICAgLi4ucmlnaHRcbiAgICAgICAgXSk7XG4gICAgICB9XG4gICAgICByZXR1cm4gcmlnaHQ7XG4gICAgfVxuICB9XG4gIHJldHVybiByaWdodDtcbn1cbi8qKlxuICogVGVzdCB3aGV0aGVyIGEgdmFsdWUgaXMgbWVyZ2VhYmxlIG9yIG5vdFxuICogQnVpbHRpbnMgdGhhdCBsb29rIGxpa2Ugb2JqZWN0cywgbnVsbCBhbmQgdXNlciBkZWZpbmVkIGNsYXNzZXNcbiAqIGFyZSBub3QgY29uc2lkZXJlZCBtZXJnZWFibGUgKGl0IG1lYW5zIHRoYXQgcmVmZXJlbmNlIHdpbGwgYmUgY29waWVkKVxuICovIGZ1bmN0aW9uIGlzTWVyZ2VhYmxlKHZhbHVlKSB7XG4gIHJldHVybiBPYmplY3QuZ2V0UHJvdG90eXBlT2YodmFsdWUpID09PSBPYmplY3QucHJvdG90eXBlO1xufVxuZnVuY3Rpb24gaXNJdGVyYWJsZSh2YWx1ZSkge1xuICByZXR1cm4gdHlwZW9mIHZhbHVlW1N5bWJvbC5pdGVyYXRvcl0gPT09IFwiZnVuY3Rpb25cIjtcbn1cbmZ1bmN0aW9uIGlzTm9uTnVsbE9iamVjdCh2YWx1ZSkge1xuICByZXR1cm4gdmFsdWUgIT09IG51bGwgJiYgdHlwZW9mIHZhbHVlID09PSBcIm9iamVjdFwiO1xufVxuZnVuY3Rpb24gZ2V0S2V5cyhyZWNvcmQpIHtcbiAgY29uc3QgcmVzdWx0ID0gT2JqZWN0LmdldE93blByb3BlcnR5U3ltYm9scyhyZWNvcmQpO1xuICBmaWx0ZXJJblBsYWNlKHJlc3VsdCwgKGtleSk9Pk9iamVjdC5wcm90b3R5cGUucHJvcGVydHlJc0VudW1lcmFibGUuY2FsbChyZWNvcmQsIGtleSkpO1xuICByZXN1bHQucHVzaCguLi5PYmplY3Qua2V5cyhyZWNvcmQpKTtcbiAgcmV0dXJuIHJlc3VsdDtcbn1cbi8vIyBzb3VyY2VNYXBwaW5nVVJMPWRlZXBfbWVyZ2UuanMubWFwIiwiLy8gQ29weXJpZ2h0IDIwMTgtMjAyNSB0aGUgRGVubyBhdXRob3JzLiBNSVQgbGljZW5zZS5cbi8vIFRoaXMgbW9kdWxlIGlzIGJyb3dzZXIgY29tcGF0aWJsZS5cbmltcG9ydCB7IGRlZXBNZXJnZSB9IGZyb20gXCJAanNyL3N0ZF9fY29sbGVjdGlvbnMvZGVlcC1tZXJnZVwiO1xuLyoqXG4gKiBDb3B5IG9mIGBpbXBvcnQgeyBpc0xlYXAgfSBmcm9tIFwiQHN0ZC9kYXRldGltZVwiO2AgYmVjYXVzZSBpdCBjYW5ub3QgYmUgaW1wb3RlZCBhcyBsb25nIGFzIGl0IGlzIHVuc3RhYmxlLlxuICovIGZ1bmN0aW9uIGlzTGVhcCh5ZWFyTnVtYmVyKSB7XG4gIHJldHVybiB5ZWFyTnVtYmVyICUgNCA9PT0gMCAmJiB5ZWFyTnVtYmVyICUgMTAwICE9PSAwIHx8IHllYXJOdW1iZXIgJSA0MDAgPT09IDA7XG59XG5leHBvcnQgY2xhc3MgU2Nhbm5lciB7XG4gICN3aGl0ZXNwYWNlID0gL1sgXFx0XS87XG4gICNwb3NpdGlvbiA9IDA7XG4gICNzb3VyY2U7XG4gIGNvbnN0cnVjdG9yKHNvdXJjZSl7XG4gICAgdGhpcy4jc291cmNlID0gc291cmNlO1xuICB9XG4gIGdldCBwb3NpdGlvbigpIHtcbiAgICByZXR1cm4gdGhpcy4jcG9zaXRpb247XG4gIH1cbiAgZ2V0IHNvdXJjZSgpIHtcbiAgICByZXR1cm4gdGhpcy4jc291cmNlO1xuICB9XG4gIC8qKlxuICAgKiBHZXQgY3VycmVudCBjaGFyYWN0ZXJcbiAgICogQHBhcmFtIGluZGV4IC0gcmVsYXRpdmUgaW5kZXggZnJvbSBjdXJyZW50IHBvc2l0aW9uXG4gICAqLyBjaGFyKGluZGV4ID0gMCkge1xuICAgIHJldHVybiB0aGlzLiNzb3VyY2VbdGhpcy4jcG9zaXRpb24gKyBpbmRleF0gPz8gXCJcIjtcbiAgfVxuICAvKipcbiAgICogR2V0IHNsaWNlZCBzdHJpbmdcbiAgICogQHBhcmFtIHN0YXJ0IC0gc3RhcnQgcG9zaXRpb24gcmVsYXRpdmUgZnJvbSBjdXJyZW50IHBvc2l0aW9uXG4gICAqIEBwYXJhbSBlbmQgLSBlbmQgcG9zaXRpb24gcmVsYXRpdmUgZnJvbSBjdXJyZW50IHBvc2l0aW9uXG4gICAqLyBzbGljZShzdGFydCwgZW5kKSB7XG4gICAgcmV0dXJuIHRoaXMuI3NvdXJjZS5zbGljZSh0aGlzLiNwb3NpdGlvbiArIHN0YXJ0LCB0aGlzLiNwb3NpdGlvbiArIGVuZCk7XG4gIH1cbiAgLyoqXG4gICAqIE1vdmUgcG9zaXRpb24gdG8gbmV4dFxuICAgKi8gbmV4dChjb3VudCA9IDEpIHtcbiAgICB0aGlzLiNwb3NpdGlvbiArPSBjb3VudDtcbiAgfVxuICBza2lwV2hpdGVzcGFjZXMoKSB7XG4gICAgd2hpbGUodGhpcy4jd2hpdGVzcGFjZS50ZXN0KHRoaXMuY2hhcigpKSAmJiAhdGhpcy5lb2YoKSl7XG4gICAgICB0aGlzLm5leHQoKTtcbiAgICB9XG4gICAgLy8gSW52YWxpZCBpZiBjdXJyZW50IGNoYXIgaXMgb3RoZXIga2luZHMgb2Ygd2hpdGVzcGFjZVxuICAgIGlmICghdGhpcy5pc0N1cnJlbnRDaGFyRU9MKCkgJiYgL1xccy8udGVzdCh0aGlzLmNoYXIoKSkpIHtcbiAgICAgIGNvbnN0IGVzY2FwZWQgPSBcIlxcXFx1XCIgKyB0aGlzLmNoYXIoKS5jaGFyQ29kZUF0KDApLnRvU3RyaW5nKDE2KTtcbiAgICAgIGNvbnN0IHBvc2l0aW9uID0gdGhpcy4jcG9zaXRpb247XG4gICAgICB0aHJvdyBuZXcgU3ludGF4RXJyb3IoYENhbm5vdCBwYXJzZSB0aGUgVE9NTDogSXQgY29udGFpbnMgaW52YWxpZCB3aGl0ZXNwYWNlIGF0IHBvc2l0aW9uICcke3Bvc2l0aW9ufSc6IFxcYCR7ZXNjYXBlZH1cXGBgKTtcbiAgICB9XG4gIH1cbiAgbmV4dFVudGlsQ2hhcihvcHRpb25zID0ge1xuICAgIHNraXBDb21tZW50czogdHJ1ZVxuICB9KSB7XG4gICAgd2hpbGUoIXRoaXMuZW9mKCkpe1xuICAgICAgY29uc3QgY2hhciA9IHRoaXMuY2hhcigpO1xuICAgICAgaWYgKHRoaXMuI3doaXRlc3BhY2UudGVzdChjaGFyKSB8fCB0aGlzLmlzQ3VycmVudENoYXJFT0woKSkge1xuICAgICAgICB0aGlzLm5leHQoKTtcbiAgICAgIH0gZWxzZSBpZiAob3B0aW9ucy5za2lwQ29tbWVudHMgJiYgdGhpcy5jaGFyKCkgPT09IFwiI1wiKSB7XG4gICAgICAgIC8vIGVudGVyaW5nIGNvbW1lbnRcbiAgICAgICAgd2hpbGUoIXRoaXMuaXNDdXJyZW50Q2hhckVPTCgpICYmICF0aGlzLmVvZigpKXtcbiAgICAgICAgICB0aGlzLm5leHQoKTtcbiAgICAgICAgfVxuICAgICAgfSBlbHNlIHtcbiAgICAgICAgYnJlYWs7XG4gICAgICB9XG4gICAgfVxuICB9XG4gIC8qKlxuICAgKiBQb3NpdGlvbiByZWFjaGVkIEVPRiBvciBub3RcbiAgICovIGVvZigpIHtcbiAgICByZXR1cm4gdGhpcy4jcG9zaXRpb24gPj0gdGhpcy4jc291cmNlLmxlbmd0aDtcbiAgfVxuICBpc0N1cnJlbnRDaGFyRU9MKCkge1xuICAgIHJldHVybiB0aGlzLmNoYXIoKSA9PT0gXCJcXG5cIiB8fCB0aGlzLnN0YXJ0c1dpdGgoXCJcXHJcXG5cIik7XG4gIH1cbiAgc3RhcnRzV2l0aChzZWFyY2hTdHJpbmcpIHtcbiAgICByZXR1cm4gdGhpcy4jc291cmNlLnN0YXJ0c1dpdGgoc2VhcmNoU3RyaW5nLCB0aGlzLiNwb3NpdGlvbik7XG4gIH1cbiAgbWF0Y2gocmVnRXhwKSB7XG4gICAgaWYgKCFyZWdFeHAuc3RpY2t5KSB7XG4gICAgICB0aHJvdyBuZXcgRXJyb3IoYFJlZ0V4cCAke3JlZ0V4cH0gZG9lcyBub3QgaGF2ZSBhIHN0aWNreSAneScgZmxhZ2ApO1xuICAgIH1cbiAgICByZWdFeHAubGFzdEluZGV4ID0gdGhpcy4jcG9zaXRpb247XG4gICAgcmV0dXJuIHRoaXMuI3NvdXJjZS5tYXRjaChyZWdFeHApO1xuICB9XG59XG4vLyAtLS0tLS0tLS0tLS0tLS0tLS0tLS0tLVxuLy8gVXRpbGl0aWVzXG4vLyAtLS0tLS0tLS0tLS0tLS0tLS0tLS0tLVxuZnVuY3Rpb24gc3VjY2Vzcyhib2R5KSB7XG4gIHJldHVybiB7XG4gICAgb2s6IHRydWUsXG4gICAgYm9keVxuICB9O1xufVxuZnVuY3Rpb24gZmFpbHVyZSgpIHtcbiAgcmV0dXJuIHtcbiAgICBvazogZmFsc2VcbiAgfTtcbn1cbi8qKlxuICogQ3JlYXRlcyBhIG5lc3RlZCBvYmplY3QgZnJvbSB0aGUga2V5cyBhbmQgdmFsdWVzLlxuICpcbiAqIGUuZy4gYHVuZmxhdChbXCJhXCIsIFwiYlwiLCBcImNcIl0sIDEpYCByZXR1cm5zIGB7IGE6IHsgYjogeyBjOiAxIH0gfSB9YFxuICovIGV4cG9ydCBmdW5jdGlvbiB1bmZsYXQoa2V5cywgdmFsdWVzID0ge1xuICBfX3Byb3RvX186IG51bGxcbn0pIHtcbiAgcmV0dXJuIGtleXMucmVkdWNlUmlnaHQoKGFjYywga2V5KT0+KHtcbiAgICAgIFtrZXldOiBhY2NcbiAgICB9KSwgdmFsdWVzKTtcbn1cbmZ1bmN0aW9uIGlzT2JqZWN0KHZhbHVlKSB7XG4gIHJldHVybiB0eXBlb2YgdmFsdWUgPT09IFwib2JqZWN0XCIgJiYgdmFsdWUgIT09IG51bGw7XG59XG5mdW5jdGlvbiBnZXRUYXJnZXRWYWx1ZSh0YXJnZXQsIGtleXMpIHtcbiAgY29uc3Qga2V5ID0ga2V5c1swXTtcbiAgaWYgKCFrZXkpIHtcbiAgICB0aHJvdyBuZXcgRXJyb3IoXCJDYW5ub3QgcGFyc2UgdGhlIFRPTUw6IGtleSBsZW5ndGggaXMgbm90IGEgcG9zaXRpdmUgbnVtYmVyXCIpO1xuICB9XG4gIHJldHVybiB0YXJnZXRba2V5XTtcbn1cbmZ1bmN0aW9uIGRlZXBBc3NpZ25UYWJsZSh0YXJnZXQsIHRhYmxlKSB7XG4gIGNvbnN0IHsga2V5cywgdHlwZSwgdmFsdWUgfSA9IHRhYmxlO1xuICBjb25zdCBjdXJyZW50VmFsdWUgPSBnZXRUYXJnZXRWYWx1ZSh0YXJnZXQsIGtleXMpO1xuICBpZiAoY3VycmVudFZhbHVlID09PSB1bmRlZmluZWQpIHtcbiAgICByZXR1cm4gT2JqZWN0LmFzc2lnbih0YXJnZXQsIHVuZmxhdChrZXlzLCB2YWx1ZSkpO1xuICB9XG4gIGlmIChBcnJheS5pc0FycmF5KGN1cnJlbnRWYWx1ZSkpIHtcbiAgICBjb25zdCBsYXN0ID0gY3VycmVudFZhbHVlLmF0KC0xKTtcbiAgICBkZWVwQXNzaWduKGxhc3QsIHtcbiAgICAgIHR5cGUsXG4gICAgICBrZXlzOiBrZXlzLnNsaWNlKDEpLFxuICAgICAgdmFsdWVcbiAgICB9KTtcbiAgICByZXR1cm4gdGFyZ2V0O1xuICB9XG4gIGlmIChpc09iamVjdChjdXJyZW50VmFsdWUpKSB7XG4gICAgZGVlcEFzc2lnbihjdXJyZW50VmFsdWUsIHtcbiAgICAgIHR5cGUsXG4gICAgICBrZXlzOiBrZXlzLnNsaWNlKDEpLFxuICAgICAgdmFsdWVcbiAgICB9KTtcbiAgICByZXR1cm4gdGFyZ2V0O1xuICB9XG4gIHRocm93IG5ldyBFcnJvcihcIlVuZXhwZWN0ZWQgYXNzaWduXCIpO1xufVxuZnVuY3Rpb24gZGVlcEFzc2lnblRhYmxlQXJyYXkodGFyZ2V0LCB0YWJsZSkge1xuICBjb25zdCB7IHR5cGUsIGtleXMsIHZhbHVlIH0gPSB0YWJsZTtcbiAgY29uc3QgY3VycmVudFZhbHVlID0gZ2V0VGFyZ2V0VmFsdWUodGFyZ2V0LCBrZXlzKTtcbiAgaWYgKGN1cnJlbnRWYWx1ZSA9PT0gdW5kZWZpbmVkKSB7XG4gICAgcmV0dXJuIE9iamVjdC5hc3NpZ24odGFyZ2V0LCB1bmZsYXQoa2V5cywgW1xuICAgICAgdmFsdWVcbiAgICBdKSk7XG4gIH1cbiAgaWYgKEFycmF5LmlzQXJyYXkoY3VycmVudFZhbHVlKSkge1xuICAgIGlmICh0YWJsZS5rZXlzLmxlbmd0aCA9PT0gMSkge1xuICAgICAgY3VycmVudFZhbHVlLnB1c2godmFsdWUpO1xuICAgIH0gZWxzZSB7XG4gICAgICBjb25zdCBsYXN0ID0gY3VycmVudFZhbHVlLmF0KC0xKTtcbiAgICAgIGRlZXBBc3NpZ24obGFzdCwge1xuICAgICAgICB0eXBlOiB0YWJsZS50eXBlLFxuICAgICAgICBrZXlzOiB0YWJsZS5rZXlzLnNsaWNlKDEpLFxuICAgICAgICB2YWx1ZTogdGFibGUudmFsdWVcbiAgICAgIH0pO1xuICAgIH1cbiAgICByZXR1cm4gdGFyZ2V0O1xuICB9XG4gIGlmIChpc09iamVjdChjdXJyZW50VmFsdWUpKSB7XG4gICAgZGVlcEFzc2lnbihjdXJyZW50VmFsdWUsIHtcbiAgICAgIHR5cGUsXG4gICAgICBrZXlzOiBrZXlzLnNsaWNlKDEpLFxuICAgICAgdmFsdWVcbiAgICB9KTtcbiAgICByZXR1cm4gdGFyZ2V0O1xuICB9XG4gIHRocm93IG5ldyBFcnJvcihcIlVuZXhwZWN0ZWQgYXNzaWduXCIpO1xufVxuZXhwb3J0IGZ1bmN0aW9uIGRlZXBBc3NpZ24odGFyZ2V0LCBib2R5KSB7XG4gIHN3aXRjaChib2R5LnR5cGUpe1xuICAgIGNhc2UgXCJCbG9ja1wiOlxuICAgICAgcmV0dXJuIGRlZXBNZXJnZSh0YXJnZXQsIGJvZHkudmFsdWUpO1xuICAgIGNhc2UgXCJUYWJsZVwiOlxuICAgICAgcmV0dXJuIGRlZXBBc3NpZ25UYWJsZSh0YXJnZXQsIGJvZHkpO1xuICAgIGNhc2UgXCJUYWJsZUFycmF5XCI6XG4gICAgICByZXR1cm4gZGVlcEFzc2lnblRhYmxlQXJyYXkodGFyZ2V0LCBib2R5KTtcbiAgfVxufVxuLy8gLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tXG4vLyBQYXJzZXIgY29tYmluYXRvcnMgYW5kIGdlbmVyYXRvcnNcbi8vIC0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLVxuLy8gZGVuby1saW50LWlnbm9yZSBuby1leHBsaWNpdC1hbnlcbmZ1bmN0aW9uIG9yKHBhcnNlcnMpIHtcbiAgcmV0dXJuIChzY2FubmVyKT0+e1xuICAgIGZvciAoY29uc3QgcGFyc2Ugb2YgcGFyc2Vycyl7XG4gICAgICBjb25zdCByZXN1bHQgPSBwYXJzZShzY2FubmVyKTtcbiAgICAgIGlmIChyZXN1bHQub2spIHJldHVybiByZXN1bHQ7XG4gICAgfVxuICAgIHJldHVybiBmYWlsdXJlKCk7XG4gIH07XG59XG4vKiogSm9pbiB0aGUgcGFyc2UgcmVzdWx0cyBvZiB0aGUgZ2l2ZW4gcGFyc2VyIGludG8gYW4gYXJyYXkuXG4gKlxuICogSWYgdGhlIHBhcnNlciBmYWlscyBhdCB0aGUgZmlyc3QgYXR0ZW1wdCwgaXQgd2lsbCByZXR1cm4gYW4gZW1wdHkgYXJyYXkuXG4gKi8gZnVuY3Rpb24gam9pbihwYXJzZXIsIHNlcGFyYXRvcikge1xuICBjb25zdCBTZXBhcmF0b3IgPSBjaGFyYWN0ZXIoc2VwYXJhdG9yKTtcbiAgcmV0dXJuIChzY2FubmVyKT0+e1xuICAgIGNvbnN0IG91dCA9IFtdO1xuICAgIGNvbnN0IGZpcnN0ID0gcGFyc2VyKHNjYW5uZXIpO1xuICAgIGlmICghZmlyc3Qub2spIHJldHVybiBzdWNjZXNzKG91dCk7XG4gICAgb3V0LnB1c2goZmlyc3QuYm9keSk7XG4gICAgd2hpbGUoIXNjYW5uZXIuZW9mKCkpe1xuICAgICAgaWYgKCFTZXBhcmF0b3Ioc2Nhbm5lcikub2spIGJyZWFrO1xuICAgICAgY29uc3QgcmVzdWx0ID0gcGFyc2VyKHNjYW5uZXIpO1xuICAgICAgaWYgKCFyZXN1bHQub2spIHtcbiAgICAgICAgdGhyb3cgbmV3IFN5bnRheEVycm9yKGBJbnZhbGlkIHRva2VuIGFmdGVyIFwiJHtzZXBhcmF0b3J9XCJgKTtcbiAgICAgIH1cbiAgICAgIG91dC5wdXNoKHJlc3VsdC5ib2R5KTtcbiAgICB9XG4gICAgcmV0dXJuIHN1Y2Nlc3Mob3V0KTtcbiAgfTtcbn1cbi8qKiBKb2luIHRoZSBwYXJzZSByZXN1bHRzIG9mIHRoZSBnaXZlbiBwYXJzZXIgaW50byBhbiBhcnJheS5cbiAqXG4gKiBUaGlzIHJlcXVpcmVzIHRoZSBwYXJzZXIgdG8gc3VjY2VlZCBhdCBsZWFzdCBvbmNlLlxuICovIGZ1bmN0aW9uIGpvaW4xKHBhcnNlciwgc2VwYXJhdG9yKSB7XG4gIGNvbnN0IFNlcGFyYXRvciA9IGNoYXJhY3RlcihzZXBhcmF0b3IpO1xuICByZXR1cm4gKHNjYW5uZXIpPT57XG4gICAgY29uc3QgZmlyc3QgPSBwYXJzZXIoc2Nhbm5lcik7XG4gICAgaWYgKCFmaXJzdC5vaykgcmV0dXJuIGZhaWx1cmUoKTtcbiAgICBjb25zdCBvdXQgPSBbXG4gICAgICBmaXJzdC5ib2R5XG4gICAgXTtcbiAgICB3aGlsZSghc2Nhbm5lci5lb2YoKSl7XG4gICAgICBpZiAoIVNlcGFyYXRvcihzY2FubmVyKS5vaykgYnJlYWs7XG4gICAgICBjb25zdCByZXN1bHQgPSBwYXJzZXIoc2Nhbm5lcik7XG4gICAgICBpZiAoIXJlc3VsdC5vaykge1xuICAgICAgICB0aHJvdyBuZXcgU3ludGF4RXJyb3IoYEludmFsaWQgdG9rZW4gYWZ0ZXIgXCIke3NlcGFyYXRvcn1cImApO1xuICAgICAgfVxuICAgICAgb3V0LnB1c2gocmVzdWx0LmJvZHkpO1xuICAgIH1cbiAgICByZXR1cm4gc3VjY2VzcyhvdXQpO1xuICB9O1xufVxuZnVuY3Rpb24ga3Yoa2V5UGFyc2VyLCBzZXBhcmF0b3IsIHZhbHVlUGFyc2VyKSB7XG4gIGNvbnN0IFNlcGFyYXRvciA9IGNoYXJhY3RlcihzZXBhcmF0b3IpO1xuICByZXR1cm4gKHNjYW5uZXIpPT57XG4gICAgY29uc3QgcG9zaXRpb24gPSBzY2FubmVyLnBvc2l0aW9uO1xuICAgIGNvbnN0IGtleSA9IGtleVBhcnNlcihzY2FubmVyKTtcbiAgICBpZiAoIWtleS5vaykgcmV0dXJuIGZhaWx1cmUoKTtcbiAgICBjb25zdCBzZXAgPSBTZXBhcmF0b3Ioc2Nhbm5lcik7XG4gICAgaWYgKCFzZXAub2spIHtcbiAgICAgIHRocm93IG5ldyBTeW50YXhFcnJvcihga2V5L3ZhbHVlIHBhaXIgZG9lc24ndCBoYXZlIFwiJHtzZXBhcmF0b3J9XCJgKTtcbiAgICB9XG4gICAgY29uc3QgdmFsdWUgPSB2YWx1ZVBhcnNlcihzY2FubmVyKTtcbiAgICBpZiAoIXZhbHVlLm9rKSB7XG4gICAgICBjb25zdCBsaW5lRW5kSW5kZXggPSBzY2FubmVyLnNvdXJjZS5pbmRleE9mKFwiXFxuXCIsIHNjYW5uZXIucG9zaXRpb24pO1xuICAgICAgY29uc3QgZW5kUG9zaXRpb24gPSBsaW5lRW5kSW5kZXggPiAwID8gbGluZUVuZEluZGV4IDogc2Nhbm5lci5zb3VyY2UubGVuZ3RoO1xuICAgICAgY29uc3QgbGluZSA9IHNjYW5uZXIuc291cmNlLnNsaWNlKHBvc2l0aW9uLCBlbmRQb3NpdGlvbik7XG4gICAgICB0aHJvdyBuZXcgU3ludGF4RXJyb3IoYENhbm5vdCBwYXJzZSB2YWx1ZSBvbiBsaW5lICcke2xpbmV9J2ApO1xuICAgIH1cbiAgICByZXR1cm4gc3VjY2Vzcyh1bmZsYXQoa2V5LmJvZHksIHZhbHVlLmJvZHkpKTtcbiAgfTtcbn1cbmZ1bmN0aW9uIG1lcmdlKHBhcnNlcikge1xuICByZXR1cm4gKHNjYW5uZXIpPT57XG4gICAgY29uc3QgcmVzdWx0ID0gcGFyc2VyKHNjYW5uZXIpO1xuICAgIGlmICghcmVzdWx0Lm9rKSByZXR1cm4gZmFpbHVyZSgpO1xuICAgIGxldCBib2R5ID0ge1xuICAgICAgX19wcm90b19fOiBudWxsXG4gICAgfTtcbiAgICBmb3IgKGNvbnN0IHJlY29yZCBvZiByZXN1bHQuYm9keSl7XG4gICAgICBpZiAodHlwZW9mIHJlY29yZCA9PT0gXCJvYmplY3RcIiAmJiByZWNvcmQgIT09IG51bGwpIHtcbiAgICAgICAgYm9keSA9IGRlZXBNZXJnZShib2R5LCByZWNvcmQpO1xuICAgICAgfVxuICAgIH1cbiAgICByZXR1cm4gc3VjY2Vzcyhib2R5KTtcbiAgfTtcbn1cbmZ1bmN0aW9uIHJlcGVhdChwYXJzZXIpIHtcbiAgcmV0dXJuIChzY2FubmVyKT0+e1xuICAgIGNvbnN0IGJvZHkgPSBbXTtcbiAgICB3aGlsZSghc2Nhbm5lci5lb2YoKSl7XG4gICAgICBjb25zdCByZXN1bHQgPSBwYXJzZXIoc2Nhbm5lcik7XG4gICAgICBpZiAoIXJlc3VsdC5vaykgYnJlYWs7XG4gICAgICBib2R5LnB1c2gocmVzdWx0LmJvZHkpO1xuICAgICAgc2Nhbm5lci5uZXh0VW50aWxDaGFyKCk7XG4gICAgfVxuICAgIGlmIChib2R5Lmxlbmd0aCA9PT0gMCkgcmV0dXJuIGZhaWx1cmUoKTtcbiAgICByZXR1cm4gc3VjY2Vzcyhib2R5KTtcbiAgfTtcbn1cbmZ1bmN0aW9uIHN1cnJvdW5kKGxlZnQsIHBhcnNlciwgcmlnaHQpIHtcbiAgY29uc3QgTGVmdCA9IGNoYXJhY3RlcihsZWZ0KTtcbiAgY29uc3QgUmlnaHQgPSBjaGFyYWN0ZXIocmlnaHQpO1xuICByZXR1cm4gKHNjYW5uZXIpPT57XG4gICAgaWYgKCFMZWZ0KHNjYW5uZXIpLm9rKSB7XG4gICAgICByZXR1cm4gZmFpbHVyZSgpO1xuICAgIH1cbiAgICBjb25zdCByZXN1bHQgPSBwYXJzZXIoc2Nhbm5lcik7XG4gICAgaWYgKCFyZXN1bHQub2spIHtcbiAgICAgIHRocm93IG5ldyBTeW50YXhFcnJvcihgSW52YWxpZCB0b2tlbiBhZnRlciBcIiR7bGVmdH1cImApO1xuICAgIH1cbiAgICBpZiAoIVJpZ2h0KHNjYW5uZXIpLm9rKSB7XG4gICAgICB0aHJvdyBuZXcgU3ludGF4RXJyb3IoYE5vdCBjbG9zZWQgYnkgXCIke3JpZ2h0fVwiIGFmdGVyIHN0YXJ0ZWQgd2l0aCBcIiR7bGVmdH1cImApO1xuICAgIH1cbiAgICByZXR1cm4gc3VjY2VzcyhyZXN1bHQuYm9keSk7XG4gIH07XG59XG5mdW5jdGlvbiBjaGFyYWN0ZXIoc3RyKSB7XG4gIHJldHVybiAoc2Nhbm5lcik9PntcbiAgICBzY2FubmVyLnNraXBXaGl0ZXNwYWNlcygpO1xuICAgIGlmICghc2Nhbm5lci5zdGFydHNXaXRoKHN0cikpIHJldHVybiBmYWlsdXJlKCk7XG4gICAgc2Nhbm5lci5uZXh0KHN0ci5sZW5ndGgpO1xuICAgIHNjYW5uZXIuc2tpcFdoaXRlc3BhY2VzKCk7XG4gICAgcmV0dXJuIHN1Y2Nlc3ModW5kZWZpbmVkKTtcbiAgfTtcbn1cbi8vIC0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tXG4vLyBQYXJzZXIgY29tcG9uZW50c1xuLy8gLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS1cbmNvbnN0IEJBUkVfS0VZX1JFR0VYUCA9IC9bQS1aYS16MC05Xy1dKy95O1xuZXhwb3J0IGZ1bmN0aW9uIGJhcmVLZXkoc2Nhbm5lcikge1xuICBzY2FubmVyLnNraXBXaGl0ZXNwYWNlcygpO1xuICBjb25zdCBrZXkgPSBzY2FubmVyLm1hdGNoKEJBUkVfS0VZX1JFR0VYUCk/LlswXTtcbiAgaWYgKCFrZXkpIHJldHVybiBmYWlsdXJlKCk7XG4gIHNjYW5uZXIubmV4dChrZXkubGVuZ3RoKTtcbiAgcmV0dXJuIHN1Y2Nlc3Moa2V5KTtcbn1cbmZ1bmN0aW9uIGVzY2FwZVNlcXVlbmNlKHNjYW5uZXIpIHtcbiAgaWYgKHNjYW5uZXIuY2hhcigpICE9PSBcIlxcXFxcIikgcmV0dXJuIGZhaWx1cmUoKTtcbiAgc2Nhbm5lci5uZXh0KCk7XG4gIC8vIFNlZSBodHRwczovL3RvbWwuaW8vZW4vdjEuMC4wLXJjLjMjc3RyaW5nXG4gIHN3aXRjaChzY2FubmVyLmNoYXIoKSl7XG4gICAgY2FzZSBcImJcIjpcbiAgICAgIHNjYW5uZXIubmV4dCgpO1xuICAgICAgcmV0dXJuIHN1Y2Nlc3MoXCJcXGJcIik7XG4gICAgY2FzZSBcInRcIjpcbiAgICAgIHNjYW5uZXIubmV4dCgpO1xuICAgICAgcmV0dXJuIHN1Y2Nlc3MoXCJcXHRcIik7XG4gICAgY2FzZSBcIm5cIjpcbiAgICAgIHNjYW5uZXIubmV4dCgpO1xuICAgICAgcmV0dXJuIHN1Y2Nlc3MoXCJcXG5cIik7XG4gICAgY2FzZSBcImZcIjpcbiAgICAgIHNjYW5uZXIubmV4dCgpO1xuICAgICAgcmV0dXJuIHN1Y2Nlc3MoXCJcXGZcIik7XG4gICAgY2FzZSBcInJcIjpcbiAgICAgIHNjYW5uZXIubmV4dCgpO1xuICAgICAgcmV0dXJuIHN1Y2Nlc3MoXCJcXHJcIik7XG4gICAgY2FzZSBcInVcIjpcbiAgICBjYXNlIFwiVVwiOlxuICAgICAge1xuICAgICAgICAvLyBVbmljb2RlIGNoYXJhY3RlclxuICAgICAgICBjb25zdCBjb2RlUG9pbnRMZW4gPSBzY2FubmVyLmNoYXIoKSA9PT0gXCJ1XCIgPyA0IDogNjtcbiAgICAgICAgY29uc3QgY29kZVBvaW50ID0gcGFyc2VJbnQoXCIweFwiICsgc2Nhbm5lci5zbGljZSgxLCAxICsgY29kZVBvaW50TGVuKSwgMTYpO1xuICAgICAgICBjb25zdCBzdHIgPSBTdHJpbmcuZnJvbUNvZGVQb2ludChjb2RlUG9pbnQpO1xuICAgICAgICBzY2FubmVyLm5leHQoY29kZVBvaW50TGVuICsgMSk7XG4gICAgICAgIHJldHVybiBzdWNjZXNzKHN0cik7XG4gICAgICB9XG4gICAgY2FzZSAnXCInOlxuICAgICAgc2Nhbm5lci5uZXh0KCk7XG4gICAgICByZXR1cm4gc3VjY2VzcygnXCInKTtcbiAgICBjYXNlIFwiXFxcXFwiOlxuICAgICAgc2Nhbm5lci5uZXh0KCk7XG4gICAgICByZXR1cm4gc3VjY2VzcyhcIlxcXFxcIik7XG4gICAgZGVmYXVsdDpcbiAgICAgIHRocm93IG5ldyBTeW50YXhFcnJvcihgSW52YWxpZCBlc2NhcGUgc2VxdWVuY2U6IFxcXFwke3NjYW5uZXIuY2hhcigpfWApO1xuICB9XG59XG5leHBvcnQgZnVuY3Rpb24gYmFzaWNTdHJpbmcoc2Nhbm5lcikge1xuICBzY2FubmVyLnNraXBXaGl0ZXNwYWNlcygpO1xuICBpZiAoc2Nhbm5lci5jaGFyKCkgIT09ICdcIicpIHJldHVybiBmYWlsdXJlKCk7XG4gIHNjYW5uZXIubmV4dCgpO1xuICBjb25zdCBhY2MgPSBbXTtcbiAgd2hpbGUoc2Nhbm5lci5jaGFyKCkgIT09ICdcIicgJiYgIXNjYW5uZXIuZW9mKCkpe1xuICAgIGlmIChzY2FubmVyLmNoYXIoKSA9PT0gXCJcXG5cIikge1xuICAgICAgdGhyb3cgbmV3IFN5bnRheEVycm9yKFwiU2luZ2xlLWxpbmUgc3RyaW5nIGNhbm5vdCBjb250YWluIEVPTFwiKTtcbiAgICB9XG4gICAgY29uc3QgZXNjYXBlZENoYXIgPSBlc2NhcGVTZXF1ZW5jZShzY2FubmVyKTtcbiAgICBpZiAoZXNjYXBlZENoYXIub2spIHtcbiAgICAgIGFjYy5wdXNoKGVzY2FwZWRDaGFyLmJvZHkpO1xuICAgIH0gZWxzZSB7XG4gICAgICBhY2MucHVzaChzY2FubmVyLmNoYXIoKSk7XG4gICAgICBzY2FubmVyLm5leHQoKTtcbiAgICB9XG4gIH1cbiAgaWYgKHNjYW5uZXIuZW9mKCkpIHtcbiAgICB0aHJvdyBuZXcgU3ludGF4RXJyb3IoYFNpbmdsZS1saW5lIHN0cmluZyBpcyBub3QgY2xvc2VkOlxcbiR7YWNjLmpvaW4oXCJcIil9YCk7XG4gIH1cbiAgc2Nhbm5lci5uZXh0KCk7IC8vIHNraXAgbGFzdCAnXCJcIlxuICByZXR1cm4gc3VjY2VzcyhhY2Muam9pbihcIlwiKSk7XG59XG5leHBvcnQgZnVuY3Rpb24gbGl0ZXJhbFN0cmluZyhzY2FubmVyKSB7XG4gIHNjYW5uZXIuc2tpcFdoaXRlc3BhY2VzKCk7XG4gIGlmIChzY2FubmVyLmNoYXIoKSAhPT0gXCInXCIpIHJldHVybiBmYWlsdXJlKCk7XG4gIHNjYW5uZXIubmV4dCgpO1xuICBjb25zdCBhY2MgPSBbXTtcbiAgd2hpbGUoc2Nhbm5lci5jaGFyKCkgIT09IFwiJ1wiICYmICFzY2FubmVyLmVvZigpKXtcbiAgICBpZiAoc2Nhbm5lci5jaGFyKCkgPT09IFwiXFxuXCIpIHtcbiAgICAgIHRocm93IG5ldyBTeW50YXhFcnJvcihcIlNpbmdsZS1saW5lIHN0cmluZyBjYW5ub3QgY29udGFpbiBFT0xcIik7XG4gICAgfVxuICAgIGFjYy5wdXNoKHNjYW5uZXIuY2hhcigpKTtcbiAgICBzY2FubmVyLm5leHQoKTtcbiAgfVxuICBpZiAoc2Nhbm5lci5lb2YoKSkge1xuICAgIHRocm93IG5ldyBTeW50YXhFcnJvcihgU2luZ2xlLWxpbmUgc3RyaW5nIGlzIG5vdCBjbG9zZWQ6XFxuJHthY2Muam9pbihcIlwiKX1gKTtcbiAgfVxuICBzY2FubmVyLm5leHQoKTsgLy8gc2tpcCBsYXN0IFwiJ1wiXG4gIHJldHVybiBzdWNjZXNzKGFjYy5qb2luKFwiXCIpKTtcbn1cbmV4cG9ydCBmdW5jdGlvbiBtdWx0aWxpbmVCYXNpY1N0cmluZyhzY2FubmVyKSB7XG4gIHNjYW5uZXIuc2tpcFdoaXRlc3BhY2VzKCk7XG4gIGlmICghc2Nhbm5lci5zdGFydHNXaXRoKCdcIlwiXCInKSkgcmV0dXJuIGZhaWx1cmUoKTtcbiAgc2Nhbm5lci5uZXh0KDMpO1xuICBpZiAoc2Nhbm5lci5jaGFyKCkgPT09IFwiXFxuXCIpIHtcbiAgICAvLyBUaGUgZmlyc3QgbmV3bGluZSAoTEYpIGlzIHRyaW1tZWRcbiAgICBzY2FubmVyLm5leHQoKTtcbiAgfSBlbHNlIGlmIChzY2FubmVyLnN0YXJ0c1dpdGgoXCJcXHJcXG5cIikpIHtcbiAgICAvLyBUaGUgZmlyc3QgbmV3bGluZSAoQ1JMRikgaXMgdHJpbW1lZFxuICAgIHNjYW5uZXIubmV4dCgyKTtcbiAgfVxuICBjb25zdCBhY2MgPSBbXTtcbiAgd2hpbGUoIXNjYW5uZXIuc3RhcnRzV2l0aCgnXCJcIlwiJykgJiYgIXNjYW5uZXIuZW9mKCkpe1xuICAgIC8vIGxpbmUgZW5kaW5nIGJhY2tzbGFzaFxuICAgIGlmIChzY2FubmVyLnN0YXJ0c1dpdGgoXCJcXFxcXFxuXCIpKSB7XG4gICAgICBzY2FubmVyLm5leHQoKTtcbiAgICAgIHNjYW5uZXIubmV4dFVudGlsQ2hhcih7XG4gICAgICAgIHNraXBDb21tZW50czogZmFsc2VcbiAgICAgIH0pO1xuICAgICAgY29udGludWU7XG4gICAgfSBlbHNlIGlmIChzY2FubmVyLnN0YXJ0c1dpdGgoXCJcXFxcXFxyXFxuXCIpKSB7XG4gICAgICBzY2FubmVyLm5leHQoKTtcbiAgICAgIHNjYW5uZXIubmV4dFVudGlsQ2hhcih7XG4gICAgICAgIHNraXBDb21tZW50czogZmFsc2VcbiAgICAgIH0pO1xuICAgICAgY29udGludWU7XG4gICAgfVxuICAgIGNvbnN0IGVzY2FwZWRDaGFyID0gZXNjYXBlU2VxdWVuY2Uoc2Nhbm5lcik7XG4gICAgaWYgKGVzY2FwZWRDaGFyLm9rKSB7XG4gICAgICBhY2MucHVzaChlc2NhcGVkQ2hhci5ib2R5KTtcbiAgICB9IGVsc2Uge1xuICAgICAgYWNjLnB1c2goc2Nhbm5lci5jaGFyKCkpO1xuICAgICAgc2Nhbm5lci5uZXh0KCk7XG4gICAgfVxuICB9XG4gIGlmIChzY2FubmVyLmVvZigpKSB7XG4gICAgdGhyb3cgbmV3IFN5bnRheEVycm9yKGBNdWx0aS1saW5lIHN0cmluZyBpcyBub3QgY2xvc2VkOlxcbiR7YWNjLmpvaW4oXCJcIil9YCk7XG4gIH1cbiAgLy8gaWYgZW5kcyB3aXRoIDQgYFwiYCwgcHVzaCB0aGUgZmlzdCBgXCJgIHRvIHN0cmluZ1xuICBpZiAoc2Nhbm5lci5jaGFyKDMpID09PSAnXCInKSB7XG4gICAgYWNjLnB1c2goJ1wiJyk7XG4gICAgc2Nhbm5lci5uZXh0KCk7XG4gIH1cbiAgc2Nhbm5lci5uZXh0KDMpOyAvLyBza2lwIGxhc3QgJ1wiXCJcIlwiXG4gIHJldHVybiBzdWNjZXNzKGFjYy5qb2luKFwiXCIpKTtcbn1cbmV4cG9ydCBmdW5jdGlvbiBtdWx0aWxpbmVMaXRlcmFsU3RyaW5nKHNjYW5uZXIpIHtcbiAgc2Nhbm5lci5za2lwV2hpdGVzcGFjZXMoKTtcbiAgaWYgKCFzY2FubmVyLnN0YXJ0c1dpdGgoXCInJydcIikpIHJldHVybiBmYWlsdXJlKCk7XG4gIHNjYW5uZXIubmV4dCgzKTtcbiAgaWYgKHNjYW5uZXIuY2hhcigpID09PSBcIlxcblwiKSB7XG4gICAgLy8gVGhlIGZpcnN0IG5ld2xpbmUgKExGKSBpcyB0cmltbWVkXG4gICAgc2Nhbm5lci5uZXh0KCk7XG4gIH0gZWxzZSBpZiAoc2Nhbm5lci5zdGFydHNXaXRoKFwiXFxyXFxuXCIpKSB7XG4gICAgLy8gVGhlIGZpcnN0IG5ld2xpbmUgKENSTEYpIGlzIHRyaW1tZWRcbiAgICBzY2FubmVyLm5leHQoMik7XG4gIH1cbiAgY29uc3QgYWNjID0gW107XG4gIHdoaWxlKCFzY2FubmVyLnN0YXJ0c1dpdGgoXCInJydcIikgJiYgIXNjYW5uZXIuZW9mKCkpe1xuICAgIGFjYy5wdXNoKHNjYW5uZXIuY2hhcigpKTtcbiAgICBzY2FubmVyLm5leHQoKTtcbiAgfVxuICBpZiAoc2Nhbm5lci5lb2YoKSkge1xuICAgIHRocm93IG5ldyBTeW50YXhFcnJvcihgTXVsdGktbGluZSBzdHJpbmcgaXMgbm90IGNsb3NlZDpcXG4ke2FjYy5qb2luKFwiXCIpfWApO1xuICB9XG4gIC8vIGlmIGVuZHMgd2l0aCA0IGAnYCwgcHVzaCB0aGUgZmlzdCBgJ2AgdG8gc3RyaW5nXG4gIGlmIChzY2FubmVyLmNoYXIoMykgPT09IFwiJ1wiKSB7XG4gICAgYWNjLnB1c2goXCInXCIpO1xuICAgIHNjYW5uZXIubmV4dCgpO1xuICB9XG4gIHNjYW5uZXIubmV4dCgzKTsgLy8gc2tpcCBsYXN0IFwiJycnXCJcbiAgcmV0dXJuIHN1Y2Nlc3MoYWNjLmpvaW4oXCJcIikpO1xufVxuY29uc3QgQk9PTEVBTl9SRUdFWFAgPSAvKD86dHJ1ZXxmYWxzZSlcXGIveTtcbmV4cG9ydCBmdW5jdGlvbiBib29sZWFuKHNjYW5uZXIpIHtcbiAgc2Nhbm5lci5za2lwV2hpdGVzcGFjZXMoKTtcbiAgY29uc3QgbWF0Y2ggPSBzY2FubmVyLm1hdGNoKEJPT0xFQU5fUkVHRVhQKTtcbiAgaWYgKCFtYXRjaCkgcmV0dXJuIGZhaWx1cmUoKTtcbiAgY29uc3Qgc3RyaW5nID0gbWF0Y2hbMF07XG4gIHNjYW5uZXIubmV4dChzdHJpbmcubGVuZ3RoKTtcbiAgY29uc3QgdmFsdWUgPSBzdHJpbmcgPT09IFwidHJ1ZVwiO1xuICByZXR1cm4gc3VjY2Vzcyh2YWx1ZSk7XG59XG5jb25zdCBJTkZJTklUWV9NQVAgPSBuZXcgTWFwKFtcbiAgW1xuICAgIFwiaW5mXCIsXG4gICAgSW5maW5pdHlcbiAgXSxcbiAgW1xuICAgIFwiK2luZlwiLFxuICAgIEluZmluaXR5XG4gIF0sXG4gIFtcbiAgICBcIi1pbmZcIixcbiAgICAtSW5maW5pdHlcbiAgXVxuXSk7XG5jb25zdCBJTkZJTklUWV9SRUdFWFAgPSAvWystXT9pbmZcXGIveTtcbmV4cG9ydCBmdW5jdGlvbiBpbmZpbml0eShzY2FubmVyKSB7XG4gIHNjYW5uZXIuc2tpcFdoaXRlc3BhY2VzKCk7XG4gIGNvbnN0IG1hdGNoID0gc2Nhbm5lci5tYXRjaChJTkZJTklUWV9SRUdFWFApO1xuICBpZiAoIW1hdGNoKSByZXR1cm4gZmFpbHVyZSgpO1xuICBjb25zdCBzdHJpbmcgPSBtYXRjaFswXTtcbiAgc2Nhbm5lci5uZXh0KHN0cmluZy5sZW5ndGgpO1xuICBjb25zdCB2YWx1ZSA9IElORklOSVRZX01BUC5nZXQoc3RyaW5nKTtcbiAgcmV0dXJuIHN1Y2Nlc3ModmFsdWUpO1xufVxuY29uc3QgTkFOX1JFR0VYUCA9IC9bKy1dP25hblxcYi95O1xuZXhwb3J0IGZ1bmN0aW9uIG5hbihzY2FubmVyKSB7XG4gIHNjYW5uZXIuc2tpcFdoaXRlc3BhY2VzKCk7XG4gIGNvbnN0IG1hdGNoID0gc2Nhbm5lci5tYXRjaChOQU5fUkVHRVhQKTtcbiAgaWYgKCFtYXRjaCkgcmV0dXJuIGZhaWx1cmUoKTtcbiAgY29uc3Qgc3RyaW5nID0gbWF0Y2hbMF07XG4gIHNjYW5uZXIubmV4dChzdHJpbmcubGVuZ3RoKTtcbiAgY29uc3QgdmFsdWUgPSBOYU47XG4gIHJldHVybiBzdWNjZXNzKHZhbHVlKTtcbn1cbmV4cG9ydCBjb25zdCBkb3R0ZWRLZXkgPSBqb2luMShvcihbXG4gIGJhcmVLZXksXG4gIGJhc2ljU3RyaW5nLFxuICBsaXRlcmFsU3RyaW5nXG5dKSwgXCIuXCIpO1xuY29uc3QgQklOQVJZX1JFR0VYUCA9IC8wYlswMV0rKD86X1swMV0rKSpcXGIveTtcbmV4cG9ydCBmdW5jdGlvbiBiaW5hcnkoc2Nhbm5lcikge1xuICBzY2FubmVyLnNraXBXaGl0ZXNwYWNlcygpO1xuICBjb25zdCBtYXRjaCA9IHNjYW5uZXIubWF0Y2goQklOQVJZX1JFR0VYUCk/LlswXTtcbiAgaWYgKCFtYXRjaCkgcmV0dXJuIGZhaWx1cmUoKTtcbiAgc2Nhbm5lci5uZXh0KG1hdGNoLmxlbmd0aCk7XG4gIGNvbnN0IHZhbHVlID0gbWF0Y2guc2xpY2UoMikucmVwbGFjZUFsbChcIl9cIiwgXCJcIik7XG4gIGNvbnN0IG51bWJlciA9IHBhcnNlSW50KHZhbHVlLCAyKTtcbiAgcmV0dXJuIGlzTmFOKG51bWJlcikgPyBmYWlsdXJlKCkgOiBzdWNjZXNzKG51bWJlcik7XG59XG5jb25zdCBPQ1RBTF9SRUdFWFAgPSAvMG9bMC03XSsoPzpfWzAtN10rKSpcXGIveTtcbmV4cG9ydCBmdW5jdGlvbiBvY3RhbChzY2FubmVyKSB7XG4gIHNjYW5uZXIuc2tpcFdoaXRlc3BhY2VzKCk7XG4gIGNvbnN0IG1hdGNoID0gc2Nhbm5lci5tYXRjaChPQ1RBTF9SRUdFWFApPy5bMF07XG4gIGlmICghbWF0Y2gpIHJldHVybiBmYWlsdXJlKCk7XG4gIHNjYW5uZXIubmV4dChtYXRjaC5sZW5ndGgpO1xuICBjb25zdCB2YWx1ZSA9IG1hdGNoLnNsaWNlKDIpLnJlcGxhY2VBbGwoXCJfXCIsIFwiXCIpO1xuICBjb25zdCBudW1iZXIgPSBwYXJzZUludCh2YWx1ZSwgOCk7XG4gIHJldHVybiBpc05hTihudW1iZXIpID8gZmFpbHVyZSgpIDogc3VjY2VzcyhudW1iZXIpO1xufVxuY29uc3QgSEVYX1JFR0VYUCA9IC8weFswLTlhLWZdKyg/Ol9bMC05YS1mXSspKlxcYi95aTtcbmV4cG9ydCBmdW5jdGlvbiBoZXgoc2Nhbm5lcikge1xuICBzY2FubmVyLnNraXBXaGl0ZXNwYWNlcygpO1xuICBjb25zdCBtYXRjaCA9IHNjYW5uZXIubWF0Y2goSEVYX1JFR0VYUCk/LlswXTtcbiAgaWYgKCFtYXRjaCkgcmV0dXJuIGZhaWx1cmUoKTtcbiAgc2Nhbm5lci5uZXh0KG1hdGNoLmxlbmd0aCk7XG4gIGNvbnN0IHZhbHVlID0gbWF0Y2guc2xpY2UoMikucmVwbGFjZUFsbChcIl9cIiwgXCJcIik7XG4gIGNvbnN0IG51bWJlciA9IHBhcnNlSW50KHZhbHVlLCAxNik7XG4gIHJldHVybiBpc05hTihudW1iZXIpID8gZmFpbHVyZSgpIDogc3VjY2VzcyhudW1iZXIpO1xufVxuY29uc3QgSU5URUdFUl9SRUdFWFAgPSAvWystXT8oPzowfFsxLTldWzAtOV0qKD86X1swLTldKykqKVxcYi95O1xuZXhwb3J0IGZ1bmN0aW9uIGludGVnZXIoc2Nhbm5lcikge1xuICBzY2FubmVyLnNraXBXaGl0ZXNwYWNlcygpO1xuICBjb25zdCBtYXRjaCA9IHNjYW5uZXIubWF0Y2goSU5URUdFUl9SRUdFWFApPy5bMF07XG4gIGlmICghbWF0Y2gpIHJldHVybiBmYWlsdXJlKCk7XG4gIHNjYW5uZXIubmV4dChtYXRjaC5sZW5ndGgpO1xuICBjb25zdCB2YWx1ZSA9IG1hdGNoLnJlcGxhY2VBbGwoXCJfXCIsIFwiXCIpO1xuICBjb25zdCBpbnQgPSBwYXJzZUludCh2YWx1ZSwgMTApO1xuICByZXR1cm4gc3VjY2VzcyhpbnQpO1xufVxuY29uc3QgRkxPQVRfUkVHRVhQID0gL1srLV0/KD86MHxbMS05XVswLTldKig/Ol9bMC05XSspKikoPzpcXC5bMC05XSsoPzpfWzAtOV0rKSopPyg/OmVbKy1dP1swLTldKyg/Ol9bMC05XSspKik/XFxiL3lpO1xuZXhwb3J0IGZ1bmN0aW9uIGZsb2F0KHNjYW5uZXIpIHtcbiAgc2Nhbm5lci5za2lwV2hpdGVzcGFjZXMoKTtcbiAgY29uc3QgbWF0Y2ggPSBzY2FubmVyLm1hdGNoKEZMT0FUX1JFR0VYUCk/LlswXTtcbiAgaWYgKCFtYXRjaCkgcmV0dXJuIGZhaWx1cmUoKTtcbiAgc2Nhbm5lci5uZXh0KG1hdGNoLmxlbmd0aCk7XG4gIGNvbnN0IHZhbHVlID0gbWF0Y2gucmVwbGFjZUFsbChcIl9cIiwgXCJcIik7XG4gIGNvbnN0IGZsb2F0ID0gcGFyc2VGbG9hdCh2YWx1ZSk7XG4gIGlmIChpc05hTihmbG9hdCkpIHJldHVybiBmYWlsdXJlKCk7XG4gIHJldHVybiBzdWNjZXNzKGZsb2F0KTtcbn1cbmNvbnN0IERBVEVfVElNRV9SRUdFWFAgPSAvKD88eWVhcj5cXGR7NH0pLSg/PG1vbnRoPlxcZHsyfSktKD88ZGF5PlxcZHsyfSkoPzpbIDAtOVRaLjorLV0rKT9cXGIveTtcbmV4cG9ydCBmdW5jdGlvbiBkYXRlVGltZShzY2FubmVyKSB7XG4gIHNjYW5uZXIuc2tpcFdoaXRlc3BhY2VzKCk7XG4gIGNvbnN0IG1hdGNoID0gc2Nhbm5lci5tYXRjaChEQVRFX1RJTUVfUkVHRVhQKTtcbiAgaWYgKCFtYXRjaCkgcmV0dXJuIGZhaWx1cmUoKTtcbiAgY29uc3Qgc3RyaW5nID0gbWF0Y2hbMF07XG4gIHNjYW5uZXIubmV4dChzdHJpbmcubGVuZ3RoKTtcbiAgY29uc3QgZ3JvdXBzID0gbWF0Y2guZ3JvdXBzO1xuICAvLyBzcGVjaWFsIGNhc2UgaWYgbW9udGggaXMgRmVicnVhcnlcbiAgaWYgKGdyb3Vwcy5tb250aCA9PSBcIjAyXCIpIHtcbiAgICBjb25zdCBkYXlzID0gcGFyc2VJbnQoZ3JvdXBzLmRheSk7XG4gICAgaWYgKGRheXMgPiAyOSkge1xuICAgICAgdGhyb3cgbmV3IFN5bnRheEVycm9yKGBJbnZhbGlkIGRhdGUgc3RyaW5nIFwiJHttYXRjaH1cImApO1xuICAgIH1cbiAgICBjb25zdCB5ZWFyID0gcGFyc2VJbnQoZ3JvdXBzLnllYXIpO1xuICAgIGlmIChkYXlzID4gMjggJiYgIWlzTGVhcCh5ZWFyKSkge1xuICAgICAgdGhyb3cgbmV3IFN5bnRheEVycm9yKGBJbnZhbGlkIGRhdGUgc3RyaW5nIFwiJHttYXRjaH1cImApO1xuICAgIH1cbiAgfVxuICBjb25zdCBkYXRlID0gbmV3IERhdGUoc3RyaW5nLnRyaW0oKSk7XG4gIC8vIGludmFsaWQgZGF0ZVxuICBpZiAoaXNOYU4oZGF0ZS5nZXRUaW1lKCkpKSB7XG4gICAgdGhyb3cgbmV3IFN5bnRheEVycm9yKGBJbnZhbGlkIGRhdGUgc3RyaW5nIFwiJHttYXRjaH1cImApO1xuICB9XG4gIHJldHVybiBzdWNjZXNzKGRhdGUpO1xufVxuY29uc3QgTE9DQUxfVElNRV9SRUdFWFAgPSAvKFxcZHsyfSk6KFxcZHsyfSk6KFxcZHsyfSkoPzpcXC5bMC05XSspP1xcYi95O1xuZXhwb3J0IGZ1bmN0aW9uIGxvY2FsVGltZShzY2FubmVyKSB7XG4gIHNjYW5uZXIuc2tpcFdoaXRlc3BhY2VzKCk7XG4gIGNvbnN0IG1hdGNoID0gc2Nhbm5lci5tYXRjaChMT0NBTF9USU1FX1JFR0VYUCk/LlswXTtcbiAgaWYgKCFtYXRjaCkgcmV0dXJuIGZhaWx1cmUoKTtcbiAgc2Nhbm5lci5uZXh0KG1hdGNoLmxlbmd0aCk7XG4gIHJldHVybiBzdWNjZXNzKG1hdGNoKTtcbn1cbmV4cG9ydCBmdW5jdGlvbiBhcnJheVZhbHVlKHNjYW5uZXIpIHtcbiAgc2Nhbm5lci5za2lwV2hpdGVzcGFjZXMoKTtcbiAgaWYgKHNjYW5uZXIuY2hhcigpICE9PSBcIltcIikgcmV0dXJuIGZhaWx1cmUoKTtcbiAgc2Nhbm5lci5uZXh0KCk7XG4gIGNvbnN0IGFycmF5ID0gW107XG4gIHdoaWxlKCFzY2FubmVyLmVvZigpKXtcbiAgICBzY2FubmVyLm5leHRVbnRpbENoYXIoKTtcbiAgICBjb25zdCByZXN1bHQgPSB2YWx1ZShzY2FubmVyKTtcbiAgICBpZiAoIXJlc3VsdC5vaykgYnJlYWs7XG4gICAgYXJyYXkucHVzaChyZXN1bHQuYm9keSk7XG4gICAgc2Nhbm5lci5za2lwV2hpdGVzcGFjZXMoKTtcbiAgICAvLyBtYXkgaGF2ZSBhIG5leHQgaXRlbSwgYnV0IHRyYWlsaW5nIGNvbW1hIGlzIGFsbG93ZWQgYXQgYXJyYXlcbiAgICBpZiAoc2Nhbm5lci5jaGFyKCkgIT09IFwiLFwiKSBicmVhaztcbiAgICBzY2FubmVyLm5leHQoKTtcbiAgfVxuICBzY2FubmVyLm5leHRVbnRpbENoYXIoKTtcbiAgaWYgKHNjYW5uZXIuY2hhcigpICE9PSBcIl1cIikgdGhyb3cgbmV3IFN5bnRheEVycm9yKFwiQXJyYXkgaXMgbm90IGNsb3NlZFwiKTtcbiAgc2Nhbm5lci5uZXh0KCk7XG4gIHJldHVybiBzdWNjZXNzKGFycmF5KTtcbn1cbmV4cG9ydCBmdW5jdGlvbiBpbmxpbmVUYWJsZShzY2FubmVyKSB7XG4gIHNjYW5uZXIubmV4dFVudGlsQ2hhcigpO1xuICBpZiAoc2Nhbm5lci5jaGFyKDEpID09PSBcIn1cIikge1xuICAgIHNjYW5uZXIubmV4dCgyKTtcbiAgICByZXR1cm4gc3VjY2Vzcyh7XG4gICAgICBfX3Byb3RvX186IG51bGxcbiAgICB9KTtcbiAgfVxuICBjb25zdCBwYWlycyA9IHN1cnJvdW5kKFwie1wiLCBqb2luKHBhaXIsIFwiLFwiKSwgXCJ9XCIpKHNjYW5uZXIpO1xuICBpZiAoIXBhaXJzLm9rKSByZXR1cm4gZmFpbHVyZSgpO1xuICBsZXQgdGFibGUgPSB7XG4gICAgX19wcm90b19fOiBudWxsXG4gIH07XG4gIGZvciAoY29uc3QgcGFpciBvZiBwYWlycy5ib2R5KXtcbiAgICB0YWJsZSA9IGRlZXBNZXJnZSh0YWJsZSwgcGFpcik7XG4gIH1cbiAgcmV0dXJuIHN1Y2Nlc3ModGFibGUpO1xufVxuZXhwb3J0IGNvbnN0IHZhbHVlID0gb3IoW1xuICBtdWx0aWxpbmVCYXNpY1N0cmluZyxcbiAgbXVsdGlsaW5lTGl0ZXJhbFN0cmluZyxcbiAgYmFzaWNTdHJpbmcsXG4gIGxpdGVyYWxTdHJpbmcsXG4gIGJvb2xlYW4sXG4gIGluZmluaXR5LFxuICBuYW4sXG4gIGRhdGVUaW1lLFxuICBsb2NhbFRpbWUsXG4gIGJpbmFyeSxcbiAgb2N0YWwsXG4gIGhleCxcbiAgZmxvYXQsXG4gIGludGVnZXIsXG4gIGFycmF5VmFsdWUsXG4gIGlubGluZVRhYmxlXG5dKTtcbmV4cG9ydCBjb25zdCBwYWlyID0ga3YoZG90dGVkS2V5LCBcIj1cIiwgdmFsdWUpO1xuZXhwb3J0IGZ1bmN0aW9uIGJsb2NrKHNjYW5uZXIpIHtcbiAgc2Nhbm5lci5uZXh0VW50aWxDaGFyKCk7XG4gIGNvbnN0IHJlc3VsdCA9IG1lcmdlKHJlcGVhdChwYWlyKSkoc2Nhbm5lcik7XG4gIGlmIChyZXN1bHQub2spIHJldHVybiBzdWNjZXNzKHtcbiAgICB0eXBlOiBcIkJsb2NrXCIsXG4gICAgdmFsdWU6IHJlc3VsdC5ib2R5XG4gIH0pO1xuICByZXR1cm4gZmFpbHVyZSgpO1xufVxuZXhwb3J0IGNvbnN0IHRhYmxlSGVhZGVyID0gc3Vycm91bmQoXCJbXCIsIGRvdHRlZEtleSwgXCJdXCIpO1xuZXhwb3J0IGZ1bmN0aW9uIHRhYmxlKHNjYW5uZXIpIHtcbiAgc2Nhbm5lci5uZXh0VW50aWxDaGFyKCk7XG4gIGNvbnN0IGhlYWRlciA9IHRhYmxlSGVhZGVyKHNjYW5uZXIpO1xuICBpZiAoIWhlYWRlci5vaykgcmV0dXJuIGZhaWx1cmUoKTtcbiAgc2Nhbm5lci5uZXh0VW50aWxDaGFyKCk7XG4gIGNvbnN0IGIgPSBibG9jayhzY2FubmVyKTtcbiAgcmV0dXJuIHN1Y2Nlc3Moe1xuICAgIHR5cGU6IFwiVGFibGVcIixcbiAgICBrZXlzOiBoZWFkZXIuYm9keSxcbiAgICB2YWx1ZTogYi5vayA/IGIuYm9keS52YWx1ZSA6IHtcbiAgICAgIF9fcHJvdG9fXzogbnVsbFxuICAgIH1cbiAgfSk7XG59XG5leHBvcnQgY29uc3QgdGFibGVBcnJheUhlYWRlciA9IHN1cnJvdW5kKFwiW1tcIiwgZG90dGVkS2V5LCBcIl1dXCIpO1xuZXhwb3J0IGZ1bmN0aW9uIHRhYmxlQXJyYXkoc2Nhbm5lcikge1xuICBzY2FubmVyLm5leHRVbnRpbENoYXIoKTtcbiAgY29uc3QgaGVhZGVyID0gdGFibGVBcnJheUhlYWRlcihzY2FubmVyKTtcbiAgaWYgKCFoZWFkZXIub2spIHJldHVybiBmYWlsdXJlKCk7XG4gIHNjYW5uZXIubmV4dFVudGlsQ2hhcigpO1xuICBjb25zdCBiID0gYmxvY2soc2Nhbm5lcik7XG4gIHJldHVybiBzdWNjZXNzKHtcbiAgICB0eXBlOiBcIlRhYmxlQXJyYXlcIixcbiAgICBrZXlzOiBoZWFkZXIuYm9keSxcbiAgICB2YWx1ZTogYi5vayA/IGIuYm9keS52YWx1ZSA6IHtcbiAgICAgIF9fcHJvdG9fXzogbnVsbFxuICAgIH1cbiAgfSk7XG59XG5leHBvcnQgZnVuY3Rpb24gdG9tbChzY2FubmVyKSB7XG4gIGNvbnN0IGJsb2NrcyA9IHJlcGVhdChvcihbXG4gICAgYmxvY2ssXG4gICAgdGFibGVBcnJheSxcbiAgICB0YWJsZVxuICBdKSkoc2Nhbm5lcik7XG4gIGlmICghYmxvY2tzLm9rKSByZXR1cm4gc3VjY2Vzcyh7XG4gICAgX19wcm90b19fOiBudWxsXG4gIH0pO1xuICBjb25zdCBib2R5ID0gYmxvY2tzLmJvZHkucmVkdWNlKGRlZXBBc3NpZ24sIHtcbiAgICBfX3Byb3RvX186IG51bGxcbiAgfSk7XG4gIHJldHVybiBzdWNjZXNzKGJvZHkpO1xufVxuZnVuY3Rpb24gY3JlYXRlUGFyc2VFcnJvck1lc3NhZ2Uoc2Nhbm5lciwgbWVzc2FnZSkge1xuICBjb25zdCBzdHJpbmcgPSBzY2FubmVyLnNvdXJjZS5zbGljZSgwLCBzY2FubmVyLnBvc2l0aW9uKTtcbiAgY29uc3QgbGluZXMgPSBzdHJpbmcuc3BsaXQoXCJcXG5cIik7XG4gIGNvbnN0IHJvdyA9IGxpbmVzLmxlbmd0aDtcbiAgY29uc3QgY29sdW1uID0gbGluZXMuYXQoLTEpPy5sZW5ndGggPz8gMDtcbiAgcmV0dXJuIGBQYXJzZSBlcnJvciBvbiBsaW5lICR7cm93fSwgY29sdW1uICR7Y29sdW1ufTogJHttZXNzYWdlfWA7XG59XG5leHBvcnQgZnVuY3Rpb24gcGFyc2VyRmFjdG9yeShwYXJzZXIpIHtcbiAgcmV0dXJuICh0b21sU3RyaW5nKT0+e1xuICAgIGNvbnN0IHNjYW5uZXIgPSBuZXcgU2Nhbm5lcih0b21sU3RyaW5nKTtcbiAgICB0cnkge1xuICAgICAgY29uc3QgcmVzdWx0ID0gcGFyc2VyKHNjYW5uZXIpO1xuICAgICAgaWYgKHJlc3VsdC5vayAmJiBzY2FubmVyLmVvZigpKSByZXR1cm4gcmVzdWx0LmJvZHk7XG4gICAgICBjb25zdCBtZXNzYWdlID0gYFVuZXhwZWN0ZWQgY2hhcmFjdGVyOiBcIiR7c2Nhbm5lci5jaGFyKCl9XCJgO1xuICAgICAgdGhyb3cgbmV3IFN5bnRheEVycm9yKGNyZWF0ZVBhcnNlRXJyb3JNZXNzYWdlKHNjYW5uZXIsIG1lc3NhZ2UpKTtcbiAgICB9IGNhdGNoIChlcnJvcikge1xuICAgICAgaWYgKGVycm9yIGluc3RhbmNlb2YgRXJyb3IpIHtcbiAgICAgICAgdGhyb3cgbmV3IFN5bnRheEVycm9yKGNyZWF0ZVBhcnNlRXJyb3JNZXNzYWdlKHNjYW5uZXIsIGVycm9yLm1lc3NhZ2UpKTtcbiAgICAgIH1cbiAgICAgIGNvbnN0IG1lc3NhZ2UgPSBcIkludmFsaWQgZXJyb3IgdHlwZSBjYXVnaHRcIjtcbiAgICAgIHRocm93IG5ldyBTeW50YXhFcnJvcihjcmVhdGVQYXJzZUVycm9yTWVzc2FnZShzY2FubmVyLCBtZXNzYWdlKSk7XG4gICAgfVxuICB9O1xufVxuLy8jIHNvdXJjZU1hcHBpbmdVUkw9X3BhcnNlci5qcy5tYXAiLCIvLyBDb3B5cmlnaHQgMjAxOC0yMDI1IHRoZSBEZW5vIGF1dGhvcnMuIE1JVCBsaWNlbnNlLlxuLy8gVGhpcyBtb2R1bGUgaXMgYnJvd3NlciBjb21wYXRpYmxlLlxuaW1wb3J0IHsgcGFyc2VyRmFjdG9yeSwgdG9tbCB9IGZyb20gXCIuL19wYXJzZXIuanNcIjtcbi8qKlxuICogUGFyc2VzIGEge0BsaW5rIGh0dHBzOi8vdG9tbC5pbyB8IFRPTUx9IHN0cmluZyBpbnRvIGFuIG9iamVjdC5cbiAqXG4gKiBAZXhhbXBsZSBVc2FnZVxuICogYGBgdHNcbiAqIGltcG9ydCB7IHBhcnNlIH0gZnJvbSBcIkBzdGQvdG9tbC9wYXJzZVwiO1xuICogaW1wb3J0IHsgYXNzZXJ0RXF1YWxzIH0gZnJvbSBcIkBzdGQvYXNzZXJ0XCI7XG4gKlxuICogY29uc3QgdG9tbFN0cmluZyA9IGB0aXRsZSA9IFwiVE9NTCBFeGFtcGxlXCJcbiAqIFtvd25lcl1cbiAqIG5hbWUgPSBcIkFsaWNlXCJcbiAqIGJpbyA9IFwiQWxpY2UgaXMgYSBwcm9ncmFtbWVyLlwiYDtcbiAqXG4gKiBjb25zdCBvYmogPSBwYXJzZSh0b21sU3RyaW5nKTtcbiAqIGFzc2VydEVxdWFscyhvYmosIHsgdGl0bGU6IFwiVE9NTCBFeGFtcGxlXCIsIG93bmVyOiB7IG5hbWU6IFwiQWxpY2VcIiwgYmlvOiBcIkFsaWNlIGlzIGEgcHJvZ3JhbW1lci5cIiB9IH0pO1xuICogYGBgXG4gKiBAcGFyYW0gdG9tbFN0cmluZyBUT01MIHN0cmluZyB0byBiZSBwYXJzZWQuXG4gKiBAcmV0dXJucyBUaGUgcGFyc2VkIEpTIG9iamVjdC5cbiAqLyBleHBvcnQgZnVuY3Rpb24gcGFyc2UodG9tbFN0cmluZykge1xuICByZXR1cm4gcGFyc2VyRmFjdG9yeSh0b21sKSh0b21sU3RyaW5nKTtcbn1cbi8vIyBzb3VyY2VNYXBwaW5nVVJMPXBhcnNlLmpzLm1hcCIsImltcG9ydCB7IGNyZWF0ZVJlcXVpcmUgfSBmcm9tIFwibm9kZTptb2R1bGVcIjtcbmltcG9ydCB7IGlzQWJzb2x1dGUsIGpvaW4sIHJlc29sdmUgfSBmcm9tIFwibm9kZTpwYXRoXCI7XG5pbXBvcnQgeyBmaWxlVVJMVG9QYXRoIH0gZnJvbSBcIm5vZGU6dXJsXCI7XG4vKipcbiogUmVzb2x2ZSBhbiBhYnNvbHV0ZSBwYXRoIGZyb20ge0BsaW5rIHJvb3R9LCBidXQgb25seVxuKiBpZiB7QGxpbmsgaW5wdXR9IGlzbid0IGFscmVhZHkgYWJzb2x1dGUuXG4qXG4qIEBwYXJhbSBpbnB1dCBUaGUgcGF0aCB0byByZXNvbHZlLlxuKiBAcGFyYW0gcm9vdCBUaGUgYmFzZSBwYXRoOyBkZWZhdWx0ID0gcHJvY2Vzcy5jd2QoKVxuKiBAcmV0dXJucyBUaGUgcmVzb2x2ZWQgYWJzb2x1dGUgcGF0aC5cbiovXG5leHBvcnQgZnVuY3Rpb24gYWJzb2x1dGUoaW5wdXQsIHJvb3QpIHtcblx0cmV0dXJuIGlzQWJzb2x1dGUoaW5wdXQpID8gaW5wdXQgOiByZXNvbHZlKHJvb3QgfHwgXCIuXCIsIGlucHV0KTtcbn1cbmV4cG9ydCBmdW5jdGlvbiBmcm9tKHJvb3QsIGlkZW50LCBzaWxlbnQpIHtcblx0dHJ5IHtcblx0XHQvLyBOT1RFOiBkaXJzIG5lZWQgYSB0cmFpbGluZyBcIi9cIiBPUiBmaWxlbmFtZS4gV2l0aCBcIi9cIiByb3V0ZSxcblx0XHQvLyBOb2RlIGFkZHMgXCJub29wLmpzXCIgYXMgbWFpbiBmaWxlLCBzbyBqdXN0IGRvIFwibm9vcC5qc1wiIGFueXdheS5cblx0XHRsZXQgciA9IHJvb3QgaW5zdGFuY2VvZiBVUkwgfHwgcm9vdC5zdGFydHNXaXRoKFwiZmlsZTovL1wiKSA/IGpvaW4oZmlsZVVSTFRvUGF0aChyb290KSwgXCJub29wLmpzXCIpIDogam9pbihhYnNvbHV0ZShyb290KSwgXCJub29wLmpzXCIpO1xuXHRcdHJldHVybiBjcmVhdGVSZXF1aXJlKHIpLnJlc29sdmUoaWRlbnQpO1xuXHR9IGNhdGNoIChlcnIpIHtcblx0XHRpZiAoIXNpbGVudCkgdGhyb3cgZXJyO1xuXHR9XG59XG5leHBvcnQgZnVuY3Rpb24gY3dkKGlkZW50LCBzaWxlbnQpIHtcblx0cmV0dXJuIGZyb20ocmVzb2x2ZSgpLCBpZGVudCwgc2lsZW50KTtcbn1cbiIsImltcG9ydCB7IGRpcm5hbWUgfSBmcm9tIFwibm9kZTpwYXRoXCI7XG5pbXBvcnQgeyBhYnNvbHV0ZSB9IGZyb20gXCJlbXBhdGhpYy9yZXNvbHZlXCI7XG4vKipcbiogR2V0IGFsbCBwYXJlbnQgZGlyZWN0b3JpZXMgb2Yge0BsaW5rIGJhc2V9LlxuKiBTdG9wcyBhZnRlciB7QGxpbmsgT3B0aW9uc1snbGFzdCddfSBpcyBwcm9jZXNzZWQuXG4qXG4qIEByZXR1cm5zIEFuIGFycmF5IG9mIGFic29sdXRlIHBhdGhzIG9mIGFsbCBwYXJlbnQgZGlyZWN0b3JpZXMuXG4qL1xuZXhwb3J0IGZ1bmN0aW9uIHVwKGJhc2UsIG9wdGlvbnMpIHtcblx0bGV0IHsgbGFzdCwgY3dkIH0gPSBvcHRpb25zIHx8IHt9O1xuXHRsZXQgdG1wID0gYWJzb2x1dGUoYmFzZSwgY3dkKTtcblx0bGV0IHJvb3QgPSBhYnNvbHV0ZShsYXN0IHx8IFwiL1wiLCBjd2QpO1xuXHRsZXQgcHJldiwgYXJyID0gW107XG5cdHdoaWxlIChwcmV2ICE9PSByb290KSB7XG5cdFx0YXJyLnB1c2godG1wKTtcblx0XHR0bXAgPSBkaXJuYW1lKHByZXYgPSB0bXApO1xuXHRcdGlmICh0bXAgPT09IHByZXYpIGJyZWFrO1xuXHR9XG5cdHJldHVybiBhcnI7XG59XG4iLCJpbXBvcnQgeyBqb2luIH0gZnJvbSBcIm5vZGU6cGF0aFwiO1xuaW1wb3J0IHsgZXhpc3RzU3luYywgc3RhdFN5bmMgfSBmcm9tIFwibm9kZTpmc1wiO1xuaW1wb3J0ICogYXMgd2FsayBmcm9tIFwiZW1wYXRoaWMvd2Fsa1wiO1xuLyoqXG4qIEZpbmQgYW4gaXRlbSBieSBuYW1lLCB3YWxraW5nIHBhcmVudCBkaXJlY3RvcmllcyB1bnRpbCBmb3VuZC5cbipcbiogQHBhcmFtIG5hbWUgVGhlIGl0ZW0gbmFtZSB0byBmaW5kLlxuKiBAcmV0dXJucyBUaGUgYWJzb2x1dGUgcGF0aCB0byB0aGUgaXRlbSwgaWYgZm91bmQuXG4qL1xuZXhwb3J0IGZ1bmN0aW9uIHVwKG5hbWUsIG9wdGlvbnMpIHtcblx0bGV0IGRpciwgdG1wO1xuXHRsZXQgc3RhcnQgPSBvcHRpb25zICYmIG9wdGlvbnMuY3dkIHx8IFwiXCI7XG5cdGZvciAoZGlyIG9mIHdhbGsudXAoc3RhcnQsIG9wdGlvbnMpKSB7XG5cdFx0dG1wID0gam9pbihkaXIsIG5hbWUpO1xuXHRcdGlmIChleGlzdHNTeW5jKHRtcCkpIHJldHVybiB0bXA7XG5cdH1cbn1cbi8qKlxuKiBHZXQgdGhlIGZpcnN0IHBhdGggdGhhdCBtYXRjaGVzIGFueSBvZiB0aGUgbmFtZXMgcHJvdmlkZWQuXG4qXG4qID4gW05PVEVdXG4qID4gVGhlIG9yZGVyIG9mIHtAbGluayBuYW1lc30gaXMgcmVzcGVjdGVkLlxuKlxuKiBAcGFyYW0gbmFtZXMgVGhlIGl0ZW0gbmFtZXMgdG8gZmluZC5cbiogQHJldHVybnMgVGhlIGFic29sdXRlIHBhdGggb2YgdGhlIGZpcnN0IGl0ZW0gZm91bmQsIGlmIGFueS5cbiovXG5leHBvcnQgZnVuY3Rpb24gYW55KG5hbWVzLCBvcHRpb25zKSB7XG5cdGxldCBkaXIsIHN0YXJ0ID0gb3B0aW9ucyAmJiBvcHRpb25zLmN3ZCB8fCBcIlwiO1xuXHRsZXQgaiA9IDAsIGxlbiA9IG5hbWVzLmxlbmd0aCwgdG1wO1xuXHRmb3IgKGRpciBvZiB3YWxrLnVwKHN0YXJ0LCBvcHRpb25zKSkge1xuXHRcdGZvciAoaiA9IDA7IGogPCBsZW47IGorKykge1xuXHRcdFx0dG1wID0gam9pbihkaXIsIG5hbWVzW2pdKTtcblx0XHRcdGlmIChleGlzdHNTeW5jKHRtcCkpIHJldHVybiB0bXA7XG5cdFx0fVxuXHR9XG59XG4vKipcbiogRmluZCBhIGZpbGUgYnkgbmFtZSwgd2Fsa2luZyBwYXJlbnQgZGlyZWN0b3JpZXMgdW50aWwgZm91bmQuXG4qXG4qID4gW05PVEVdXG4qID4gVGhpcyBmdW5jdGlvbiBvbmx5IHJldHVybnMgYSB2YWx1ZSBmb3IgZmlsZSBtYXRjaGVzLlxuKiA+IEEgZGlyZWN0b3J5IG1hdGNoIHdpdGggdGhlIHNhbWUgbmFtZSB3aWxsIGJlIGlnbm9yZWQuXG4qXG4qIEBwYXJhbSBuYW1lIFRoZSBmaWxlIG5hbWUgdG8gZmluZC5cbiogQHJldHVybnMgVGhlIGFic29sdXRlIHBhdGggdG8gdGhlIGZpbGUsIGlmIGZvdW5kLlxuKi9cbmV4cG9ydCBmdW5jdGlvbiBmaWxlKG5hbWUsIG9wdGlvbnMpIHtcblx0bGV0IGRpciwgdG1wO1xuXHRsZXQgc3RhcnQgPSBvcHRpb25zICYmIG9wdGlvbnMuY3dkIHx8IFwiXCI7XG5cdGZvciAoZGlyIG9mIHdhbGsudXAoc3RhcnQsIG9wdGlvbnMpKSB7XG5cdFx0dHJ5IHtcblx0XHRcdHRtcCA9IGpvaW4oZGlyLCBuYW1lKTtcblx0XHRcdGlmIChzdGF0U3luYyh0bXApLmlzRmlsZSgpKSByZXR1cm4gdG1wO1xuXHRcdH0gY2F0Y2gge31cblx0fVxufVxuLyoqXG4qIEZpbmQgYSBkaXJlY3RvcnkgYnkgbmFtZSwgd2Fsa2luZyBwYXJlbnQgZGlyZWN0b3JpZXMgdW50aWwgZm91bmQuXG4qXG4qID4gW05PVEVdXG4qID4gVGhpcyBmdW5jdGlvbiBvbmx5IHJldHVybnMgYSB2YWx1ZSBmb3IgZGlyZWN0b3J5IG1hdGNoZXMuXG4qID4gQSBmaWxlIG1hdGNoIHdpdGggdGhlIHNhbWUgbmFtZSB3aWxsIGJlIGlnbm9yZWQuXG4qXG4qIEBwYXJhbSBuYW1lIFRoZSBkaXJlY3RvcnkgbmFtZSB0byBmaW5kLlxuKiBAcmV0dXJucyBUaGUgYWJzb2x1dGUgcGF0aCB0byB0aGUgZmlsZSwgaWYgZm91bmQuXG4qL1xuZXhwb3J0IGZ1bmN0aW9uIGRpcihuYW1lLCBvcHRpb25zKSB7XG5cdGxldCBkaXIsIHRtcDtcblx0bGV0IHN0YXJ0ID0gb3B0aW9ucyAmJiBvcHRpb25zLmN3ZCB8fCBcIlwiO1xuXHRmb3IgKGRpciBvZiB3YWxrLnVwKHN0YXJ0LCBvcHRpb25zKSkge1xuXHRcdHRyeSB7XG5cdFx0XHR0bXAgPSBqb2luKGRpciwgbmFtZSk7XG5cdFx0XHRpZiAoc3RhdFN5bmModG1wKS5pc0RpcmVjdG9yeSgpKSByZXR1cm4gdG1wO1xuXHRcdH0gY2F0Y2gge31cblx0fVxufVxuIiwiLy8gVGhpcyBmaWxlIGlzIGdlbmVyYXRlZCBieSBjb2RlZ2VuL2luZGV4LnRzXG4vLyBEbyBub3QgZWRpdCB0aGlzIGZpbGUgbWFudWFsbHlcbmltcG9ydCB7IENvbW1hbmQsIE9wdGlvbiB9IGZyb20gJ2NsaXBhbmlvbidcblxuZXhwb3J0IGFic3RyYWN0IGNsYXNzIEJhc2VSZW5hbWVDb21tYW5kIGV4dGVuZHMgQ29tbWFuZCB7XG4gIHN0YXRpYyBwYXRocyA9IFtbJ3JlbmFtZSddXVxuXG4gIHN0YXRpYyB1c2FnZSA9IENvbW1hbmQuVXNhZ2Uoe1xuICAgIGRlc2NyaXB0aW9uOiAnUmVuYW1lIHRoZSBOQVBJLVJTIHByb2plY3QnLFxuICB9KVxuXG4gIGN3ZCA9IE9wdGlvbi5TdHJpbmcoJy0tY3dkJywgcHJvY2Vzcy5jd2QoKSwge1xuICAgIGRlc2NyaXB0aW9uOlxuICAgICAgJ1RoZSB3b3JraW5nIGRpcmVjdG9yeSBvZiB3aGVyZSBuYXBpIGNvbW1hbmQgd2lsbCBiZSBleGVjdXRlZCBpbiwgYWxsIG90aGVyIHBhdGhzIG9wdGlvbnMgYXJlIHJlbGF0aXZlIHRvIHRoaXMgcGF0aCcsXG4gIH0pXG5cbiAgY29uZmlnUGF0aD86IHN0cmluZyA9IE9wdGlvbi5TdHJpbmcoJy0tY29uZmlnLXBhdGgsLWMnLCB7XG4gICAgZGVzY3JpcHRpb246ICdQYXRoIHRvIGBuYXBpYCBjb25maWcganNvbiBmaWxlJyxcbiAgfSlcblxuICBwYWNrYWdlSnNvblBhdGggPSBPcHRpb24uU3RyaW5nKCctLXBhY2thZ2UtanNvbi1wYXRoJywgJ3BhY2thZ2UuanNvbicsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1BhdGggdG8gYHBhY2thZ2UuanNvbmAnLFxuICB9KVxuXG4gIG5wbURpciA9IE9wdGlvbi5TdHJpbmcoJy0tbnBtLWRpcicsICducG0nLCB7XG4gICAgZGVzY3JpcHRpb246ICdQYXRoIHRvIHRoZSBmb2xkZXIgd2hlcmUgdGhlIG5wbSBwYWNrYWdlcyBwdXQnLFxuICB9KVxuXG4gICQkbmFtZT86IHN0cmluZyA9IE9wdGlvbi5TdHJpbmcoJy0tbmFtZSwtbicsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1RoZSBuZXcgbmFtZSBvZiB0aGUgcHJvamVjdCcsXG4gIH0pXG5cbiAgYmluYXJ5TmFtZT86IHN0cmluZyA9IE9wdGlvbi5TdHJpbmcoJy0tYmluYXJ5LW5hbWUsLWInLCB7XG4gICAgZGVzY3JpcHRpb246ICdUaGUgbmV3IGJpbmFyeSBuYW1lICoubm9kZSBmaWxlcycsXG4gIH0pXG5cbiAgcGFja2FnZU5hbWU/OiBzdHJpbmcgPSBPcHRpb24uU3RyaW5nKCctLXBhY2thZ2UtbmFtZScsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1RoZSBuZXcgcGFja2FnZSBuYW1lIG9mIHRoZSBwcm9qZWN0JyxcbiAgfSlcblxuICBtYW5pZmVzdFBhdGggPSBPcHRpb24uU3RyaW5nKCctLW1hbmlmZXN0LXBhdGgnLCAnQ2FyZ28udG9tbCcsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1BhdGggdG8gYENhcmdvLnRvbWxgJyxcbiAgfSlcblxuICByZXBvc2l0b3J5Pzogc3RyaW5nID0gT3B0aW9uLlN0cmluZygnLS1yZXBvc2l0b3J5Jywge1xuICAgIGRlc2NyaXB0aW9uOiAnVGhlIG5ldyByZXBvc2l0b3J5IG9mIHRoZSBwcm9qZWN0JyxcbiAgfSlcblxuICBkZXNjcmlwdGlvbj86IHN0cmluZyA9IE9wdGlvbi5TdHJpbmcoJy0tZGVzY3JpcHRpb24nLCB7XG4gICAgZGVzY3JpcHRpb246ICdUaGUgbmV3IGRlc2NyaXB0aW9uIG9mIHRoZSBwcm9qZWN0JyxcbiAgfSlcblxuICBnZXRPcHRpb25zKCkge1xuICAgIHJldHVybiB7XG4gICAgICBjd2Q6IHRoaXMuY3dkLFxuICAgICAgY29uZmlnUGF0aDogdGhpcy5jb25maWdQYXRoLFxuICAgICAgcGFja2FnZUpzb25QYXRoOiB0aGlzLnBhY2thZ2VKc29uUGF0aCxcbiAgICAgIG5wbURpcjogdGhpcy5ucG1EaXIsXG4gICAgICBuYW1lOiB0aGlzLiQkbmFtZSxcbiAgICAgIGJpbmFyeU5hbWU6IHRoaXMuYmluYXJ5TmFtZSxcbiAgICAgIHBhY2thZ2VOYW1lOiB0aGlzLnBhY2thZ2VOYW1lLFxuICAgICAgbWFuaWZlc3RQYXRoOiB0aGlzLm1hbmlmZXN0UGF0aCxcbiAgICAgIHJlcG9zaXRvcnk6IHRoaXMucmVwb3NpdG9yeSxcbiAgICAgIGRlc2NyaXB0aW9uOiB0aGlzLmRlc2NyaXB0aW9uLFxuICAgIH1cbiAgfVxufVxuXG4vKipcbiAqIFJlbmFtZSB0aGUgTkFQSS1SUyBwcm9qZWN0XG4gKi9cbmV4cG9ydCBpbnRlcmZhY2UgUmVuYW1lT3B0aW9ucyB7XG4gIC8qKlxuICAgKiBUaGUgd29ya2luZyBkaXJlY3Rvcnkgb2Ygd2hlcmUgbmFwaSBjb21tYW5kIHdpbGwgYmUgZXhlY3V0ZWQgaW4sIGFsbCBvdGhlciBwYXRocyBvcHRpb25zIGFyZSByZWxhdGl2ZSB0byB0aGlzIHBhdGhcbiAgICpcbiAgICogQGRlZmF1bHQgcHJvY2Vzcy5jd2QoKVxuICAgKi9cbiAgY3dkPzogc3RyaW5nXG4gIC8qKlxuICAgKiBQYXRoIHRvIGBuYXBpYCBjb25maWcganNvbiBmaWxlXG4gICAqL1xuICBjb25maWdQYXRoPzogc3RyaW5nXG4gIC8qKlxuICAgKiBQYXRoIHRvIGBwYWNrYWdlLmpzb25gXG4gICAqXG4gICAqIEBkZWZhdWx0ICdwYWNrYWdlLmpzb24nXG4gICAqL1xuICBwYWNrYWdlSnNvblBhdGg/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gdGhlIGZvbGRlciB3aGVyZSB0aGUgbnBtIHBhY2thZ2VzIHB1dFxuICAgKlxuICAgKiBAZGVmYXVsdCAnbnBtJ1xuICAgKi9cbiAgbnBtRGlyPzogc3RyaW5nXG4gIC8qKlxuICAgKiBUaGUgbmV3IG5hbWUgb2YgdGhlIHByb2plY3RcbiAgICovXG4gIG5hbWU/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFRoZSBuZXcgYmluYXJ5IG5hbWUgKi5ub2RlIGZpbGVzXG4gICAqL1xuICBiaW5hcnlOYW1lPzogc3RyaW5nXG4gIC8qKlxuICAgKiBUaGUgbmV3IHBhY2thZ2UgbmFtZSBvZiB0aGUgcHJvamVjdFxuICAgKi9cbiAgcGFja2FnZU5hbWU/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gYENhcmdvLnRvbWxgXG4gICAqXG4gICAqIEBkZWZhdWx0ICdDYXJnby50b21sJ1xuICAgKi9cbiAgbWFuaWZlc3RQYXRoPzogc3RyaW5nXG4gIC8qKlxuICAgKiBUaGUgbmV3IHJlcG9zaXRvcnkgb2YgdGhlIHByb2plY3RcbiAgICovXG4gIHJlcG9zaXRvcnk/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFRoZSBuZXcgZGVzY3JpcHRpb24gb2YgdGhlIHByb2plY3RcbiAgICovXG4gIGRlc2NyaXB0aW9uPzogc3RyaW5nXG59XG5cbmV4cG9ydCBmdW5jdGlvbiBhcHBseURlZmF1bHRSZW5hbWVPcHRpb25zKG9wdGlvbnM6IFJlbmFtZU9wdGlvbnMpIHtcbiAgcmV0dXJuIHtcbiAgICBjd2Q6IHByb2Nlc3MuY3dkKCksXG4gICAgcGFja2FnZUpzb25QYXRoOiAncGFja2FnZS5qc29uJyxcbiAgICBucG1EaXI6ICducG0nLFxuICAgIG1hbmlmZXN0UGF0aDogJ0NhcmdvLnRvbWwnLFxuICAgIC4uLm9wdGlvbnMsXG4gIH1cbn1cbiIsImltcG9ydCB7IGV4aXN0c1N5bmMgfSBmcm9tICdub2RlOmZzJ1xuaW1wb3J0IHsgcmVuYW1lIH0gZnJvbSAnbm9kZTpmcy9wcm9taXNlcydcbmltcG9ydCB7IHJlc29sdmUsIGpvaW4gfSBmcm9tICdub2RlOnBhdGgnXG5cbmltcG9ydCB7IHBhcnNlIGFzIHBhcnNlVG9tbCwgc3RyaW5naWZ5IGFzIHN0cmluZ2lmeVRvbWwgfSBmcm9tICdAc3RkL3RvbWwnXG5pbXBvcnQgeyBsb2FkIGFzIHlhbWxQYXJzZSwgZHVtcCBhcyB5YW1sU3RyaW5naWZ5IH0gZnJvbSAnanMteWFtbCdcbmltcG9ydCB7IGlzTmlsLCBtZXJnZSwgb21pdEJ5LCBwaWNrIH0gZnJvbSAnZXMtdG9vbGtpdCdcbmltcG9ydCAqIGFzIGZpbmQgZnJvbSAnZW1wYXRoaWMvZmluZCdcblxuaW1wb3J0IHsgYXBwbHlEZWZhdWx0UmVuYW1lT3B0aW9ucywgdHlwZSBSZW5hbWVPcHRpb25zIH0gZnJvbSAnLi4vZGVmL3JlbmFtZS5qcydcbmltcG9ydCB7IHJlYWRDb25maWcsIHJlYWRGaWxlQXN5bmMsIHdyaXRlRmlsZUFzeW5jIH0gZnJvbSAnLi4vdXRpbHMvaW5kZXguanMnXG5cbmV4cG9ydCBhc3luYyBmdW5jdGlvbiByZW5hbWVQcm9qZWN0KHVzZXJPcHRpb25zOiBSZW5hbWVPcHRpb25zKSB7XG4gIGNvbnN0IG9wdGlvbnMgPSBhcHBseURlZmF1bHRSZW5hbWVPcHRpb25zKHVzZXJPcHRpb25zKVxuICBjb25zdCBuYXBpQ29uZmlnID0gYXdhaXQgcmVhZENvbmZpZyhvcHRpb25zKVxuICBjb25zdCBvbGROYW1lID0gbmFwaUNvbmZpZy5iaW5hcnlOYW1lXG5cbiAgY29uc3QgcGFja2FnZUpzb25QYXRoID0gcmVzb2x2ZShvcHRpb25zLmN3ZCwgb3B0aW9ucy5wYWNrYWdlSnNvblBhdGgpXG4gIGNvbnN0IGNhcmdvVG9tbFBhdGggPSByZXNvbHZlKG9wdGlvbnMuY3dkLCBvcHRpb25zLm1hbmlmZXN0UGF0aClcblxuICBjb25zdCBwYWNrYWdlSnNvbkNvbnRlbnQgPSBhd2FpdCByZWFkRmlsZUFzeW5jKHBhY2thZ2VKc29uUGF0aCwgJ3V0ZjgnKVxuICBjb25zdCBwYWNrYWdlSnNvbkRhdGEgPSBKU09OLnBhcnNlKHBhY2thZ2VKc29uQ29udGVudClcblxuICBtZXJnZShcbiAgICBtZXJnZShcbiAgICAgIHBhY2thZ2VKc29uRGF0YSxcbiAgICAgIG9taXRCeShcbiAgICAgICAgLy8gQHRzLWV4cGVjdC1lcnJvciBtaXNzaW5nIGZpZWxkczogYXV0aG9yIGFuZCBsaWNlbnNlXG4gICAgICAgIHBpY2sob3B0aW9ucywgWyduYW1lJywgJ2Rlc2NyaXB0aW9uJywgJ2F1dGhvcicsICdsaWNlbnNlJ10pLFxuICAgICAgICBpc05pbCxcbiAgICAgICksXG4gICAgKSxcbiAgICB7XG4gICAgICBuYXBpOiBvbWl0QnkoXG4gICAgICAgIHtcbiAgICAgICAgICBiaW5hcnlOYW1lOiBvcHRpb25zLmJpbmFyeU5hbWUsXG4gICAgICAgICAgcGFja2FnZU5hbWU6IG9wdGlvbnMucGFja2FnZU5hbWUsXG4gICAgICAgIH0sXG4gICAgICAgIGlzTmlsLFxuICAgICAgKSxcbiAgICB9LFxuICApXG5cbiAgaWYgKG9wdGlvbnMuY29uZmlnUGF0aCkge1xuICAgIGNvbnN0IGNvbmZpZ1BhdGggPSByZXNvbHZlKG9wdGlvbnMuY3dkLCBvcHRpb25zLmNvbmZpZ1BhdGgpXG4gICAgY29uc3QgY29uZmlnQ29udGVudCA9IGF3YWl0IHJlYWRGaWxlQXN5bmMoY29uZmlnUGF0aCwgJ3V0ZjgnKVxuICAgIGNvbnN0IGNvbmZpZ0RhdGEgPSBKU09OLnBhcnNlKGNvbmZpZ0NvbnRlbnQpXG4gICAgY29uZmlnRGF0YS5iaW5hcnlOYW1lID0gb3B0aW9ucy5iaW5hcnlOYW1lXG4gICAgY29uZmlnRGF0YS5wYWNrYWdlTmFtZSA9IG9wdGlvbnMucGFja2FnZU5hbWVcbiAgICBhd2FpdCB3cml0ZUZpbGVBc3luYyhjb25maWdQYXRoLCBKU09OLnN0cmluZ2lmeShjb25maWdEYXRhLCBudWxsLCAyKSlcbiAgfVxuXG4gIGF3YWl0IHdyaXRlRmlsZUFzeW5jKFxuICAgIHBhY2thZ2VKc29uUGF0aCxcbiAgICBKU09OLnN0cmluZ2lmeShwYWNrYWdlSnNvbkRhdGEsIG51bGwsIDIpLFxuICApXG5cbiAgY29uc3QgdG9tbENvbnRlbnQgPSBhd2FpdCByZWFkRmlsZUFzeW5jKGNhcmdvVG9tbFBhdGgsICd1dGY4JylcbiAgY29uc3QgY2FyZ29Ub21sID0gcGFyc2VUb21sKHRvbWxDb250ZW50KSBhcyBhbnlcblxuICAvLyBVcGRhdGUgdGhlIHBhY2thZ2UgbmFtZVxuICBpZiAoY2FyZ29Ub21sLnBhY2thZ2UgJiYgb3B0aW9ucy5iaW5hcnlOYW1lKSB7XG4gICAgLy8gU2FuaXRpemUgdGhlIGJpbmFyeSBuYW1lIGZvciBSdXN0IHBhY2thZ2UgbmFtaW5nIGNvbnZlbnRpb25zXG4gICAgY29uc3Qgc2FuaXRpemVkTmFtZSA9IG9wdGlvbnMuYmluYXJ5TmFtZVxuICAgICAgLnJlcGxhY2UoJ0AnLCAnJylcbiAgICAgIC5yZXBsYWNlKCcvJywgJ18nKVxuICAgICAgLnJlcGxhY2UoLy0vZywgJ18nKVxuICAgICAgLnRvTG93ZXJDYXNlKClcbiAgICBjYXJnb1RvbWwucGFja2FnZS5uYW1lID0gc2FuaXRpemVkTmFtZVxuICB9XG5cbiAgLy8gU3RyaW5naWZ5IHRoZSB1cGRhdGVkIFRPTUxcbiAgY29uc3QgdXBkYXRlZFRvbWxDb250ZW50ID0gc3RyaW5naWZ5VG9tbChjYXJnb1RvbWwpXG5cbiAgYXdhaXQgd3JpdGVGaWxlQXN5bmMoY2FyZ29Ub21sUGF0aCwgdXBkYXRlZFRvbWxDb250ZW50KVxuICBpZiAob2xkTmFtZSAhPT0gb3B0aW9ucy5iaW5hcnlOYW1lKSB7XG4gICAgY29uc3QgZ2l0aHViQWN0aW9uc1BhdGggPSBmaW5kLmRpcignLmdpdGh1YicsIHtcbiAgICAgIGN3ZDogb3B0aW9ucy5jd2QsXG4gICAgfSlcbiAgICBpZiAoZ2l0aHViQWN0aW9uc1BhdGgpIHtcbiAgICAgIGNvbnN0IGdpdGh1YkFjdGlvbnNDSVltbFBhdGggPSBqb2luKFxuICAgICAgICBnaXRodWJBY3Rpb25zUGF0aCxcbiAgICAgICAgJ3dvcmtmbG93cycsXG4gICAgICAgICdDSS55bWwnLFxuICAgICAgKVxuICAgICAgaWYgKGV4aXN0c1N5bmMoZ2l0aHViQWN0aW9uc0NJWW1sUGF0aCkpIHtcbiAgICAgICAgY29uc3QgZ2l0aHViQWN0aW9uc0NvbnRlbnQgPSBhd2FpdCByZWFkRmlsZUFzeW5jKFxuICAgICAgICAgIGdpdGh1YkFjdGlvbnNDSVltbFBhdGgsXG4gICAgICAgICAgJ3V0ZjgnLFxuICAgICAgICApXG4gICAgICAgIGNvbnN0IGdpdGh1YkFjdGlvbnNEYXRhID0geWFtbFBhcnNlKGdpdGh1YkFjdGlvbnNDb250ZW50KSBhcyBhbnlcbiAgICAgICAgaWYgKGdpdGh1YkFjdGlvbnNEYXRhLmVudj8uQVBQX05BTUUpIHtcbiAgICAgICAgICBnaXRodWJBY3Rpb25zRGF0YS5lbnYuQVBQX05BTUUgPSBvcHRpb25zLmJpbmFyeU5hbWVcbiAgICAgICAgICBhd2FpdCB3cml0ZUZpbGVBc3luYyhcbiAgICAgICAgICAgIGdpdGh1YkFjdGlvbnNDSVltbFBhdGgsXG4gICAgICAgICAgICB5YW1sU3RyaW5naWZ5KGdpdGh1YkFjdGlvbnNEYXRhLCB7XG4gICAgICAgICAgICAgIGxpbmVXaWR0aDogLTEsXG4gICAgICAgICAgICAgIG5vUmVmczogdHJ1ZSxcbiAgICAgICAgICAgICAgc29ydEtleXM6IGZhbHNlLFxuICAgICAgICAgICAgfSksXG4gICAgICAgICAgKVxuICAgICAgICB9XG4gICAgICB9XG4gICAgfVxuICAgIGNvbnN0IG9sZFdhc2lCcm93c2VyQmluZGluZ1BhdGggPSBqb2luKFxuICAgICAgb3B0aW9ucy5jd2QsXG4gICAgICBgJHtvbGROYW1lfS53YXNpLWJyb3dzZXIuanNgLFxuICAgIClcbiAgICBpZiAoZXhpc3RzU3luYyhvbGRXYXNpQnJvd3NlckJpbmRpbmdQYXRoKSkge1xuICAgICAgYXdhaXQgcmVuYW1lKFxuICAgICAgICBvbGRXYXNpQnJvd3NlckJpbmRpbmdQYXRoLFxuICAgICAgICBqb2luKG9wdGlvbnMuY3dkLCBgJHtvcHRpb25zLmJpbmFyeU5hbWV9Lndhc2ktYnJvd3Nlci5qc2ApLFxuICAgICAgKVxuICAgIH1cbiAgICBjb25zdCBvbGRXYXNpQmluZGluZ1BhdGggPSBqb2luKG9wdGlvbnMuY3dkLCBgJHtvbGROYW1lfS53YXNpLmNqc2ApXG4gICAgaWYgKGV4aXN0c1N5bmMob2xkV2FzaUJpbmRpbmdQYXRoKSkge1xuICAgICAgYXdhaXQgcmVuYW1lKFxuICAgICAgICBvbGRXYXNpQmluZGluZ1BhdGgsXG4gICAgICAgIGpvaW4ob3B0aW9ucy5jd2QsIGAke29wdGlvbnMuYmluYXJ5TmFtZX0ud2FzaS5janNgKSxcbiAgICAgIClcbiAgICB9XG4gICAgY29uc3QgZ2l0QXR0cmlidXRlc1BhdGggPSBqb2luKG9wdGlvbnMuY3dkLCAnLmdpdGF0dHJpYnV0ZXMnKVxuICAgIGlmIChleGlzdHNTeW5jKGdpdEF0dHJpYnV0ZXNQYXRoKSkge1xuICAgICAgY29uc3QgZ2l0QXR0cmlidXRlc0NvbnRlbnQgPSBhd2FpdCByZWFkRmlsZUFzeW5jKFxuICAgICAgICBnaXRBdHRyaWJ1dGVzUGF0aCxcbiAgICAgICAgJ3V0ZjgnLFxuICAgICAgKVxuICAgICAgY29uc3QgZ2l0QXR0cmlidXRlc0RhdGEgPSBnaXRBdHRyaWJ1dGVzQ29udGVudFxuICAgICAgICAuc3BsaXQoJ1xcbicpXG4gICAgICAgIC5tYXAoKGxpbmUpID0+IHtcbiAgICAgICAgICByZXR1cm4gbGluZVxuICAgICAgICAgICAgLnJlcGxhY2UoXG4gICAgICAgICAgICAgIGAke29sZE5hbWV9Lndhc2ktYnJvd3Nlci5qc2AsXG4gICAgICAgICAgICAgIGAke29wdGlvbnMuYmluYXJ5TmFtZX0ud2FzaS1icm93c2VyLmpzYCxcbiAgICAgICAgICAgIClcbiAgICAgICAgICAgIC5yZXBsYWNlKGAke29sZE5hbWV9Lndhc2kuY2pzYCwgYCR7b3B0aW9ucy5iaW5hcnlOYW1lfS53YXNpLmNqc2ApXG4gICAgICAgIH0pXG4gICAgICAgIC5qb2luKCdcXG4nKVxuICAgICAgYXdhaXQgd3JpdGVGaWxlQXN5bmMoZ2l0QXR0cmlidXRlc1BhdGgsIGdpdEF0dHJpYnV0ZXNEYXRhKVxuICAgIH1cbiAgfVxufVxuIiwiaW1wb3J0IHsgZXhlYywgZXhlY1N5bmMgfSBmcm9tICdub2RlOmNoaWxkX3Byb2Nlc3MnXG5pbXBvcnQgeyBleGlzdHNTeW5jIH0gZnJvbSAnbm9kZTpmcydcbmltcG9ydCB7IGhvbWVkaXIgfSBmcm9tICdub2RlOm9zJ1xuaW1wb3J0IHBhdGggZnJvbSAnbm9kZTpwYXRoJ1xuaW1wb3J0IHsgcHJvbWlzZXMgYXMgZnMgfSBmcm9tICdub2RlOmZzJ1xuXG5pbXBvcnQgeyBsb2FkIGFzIHlhbWxMb2FkLCBkdW1wIGFzIHlhbWxEdW1wIH0gZnJvbSAnanMteWFtbCdcblxuaW1wb3J0IHtcbiAgYXBwbHlEZWZhdWx0TmV3T3B0aW9ucyxcbiAgdHlwZSBOZXdPcHRpb25zIGFzIFJhd05ld09wdGlvbnMsXG59IGZyb20gJy4uL2RlZi9uZXcuanMnXG5pbXBvcnQge1xuICBBVkFJTEFCTEVfVEFSR0VUUyxcbiAgZGVidWdGYWN0b3J5LFxuICBERUZBVUxUX1RBUkdFVFMsXG4gIG1rZGlyQXN5bmMsXG4gIHJlYWRkaXJBc3luYyxcbiAgc3RhdEFzeW5jLFxuICB0eXBlIFN1cHBvcnRlZFBhY2thZ2VNYW5hZ2VyLFxufSBmcm9tICcuLi91dGlscy9pbmRleC5qcydcbmltcG9ydCB7IG5hcGlFbmdpbmVSZXF1aXJlbWVudCB9IGZyb20gJy4uL3V0aWxzL3ZlcnNpb24uanMnXG5pbXBvcnQgeyByZW5hbWVQcm9qZWN0IH0gZnJvbSAnLi9yZW5hbWUuanMnXG5cbi8vIFRlbXBsYXRlIGltcG9ydHMgcmVtb3ZlZCBhcyB3ZSdyZSBub3cgdXNpbmcgZXh0ZXJuYWwgdGVtcGxhdGVzXG5cbmNvbnN0IGRlYnVnID0gZGVidWdGYWN0b3J5KCduZXcnKVxuXG50eXBlIE5ld09wdGlvbnMgPSBSZXF1aXJlZDxSYXdOZXdPcHRpb25zPlxuXG5jb25zdCBURU1QTEFURV9SRVBPUyA9IHtcbiAgeWFybjogJ2h0dHBzOi8vZ2l0aHViLmNvbS9uYXBpLXJzL3BhY2thZ2UtdGVtcGxhdGUnLFxuICBwbnBtOiAnaHR0cHM6Ly9naXRodWIuY29tL25hcGktcnMvcGFja2FnZS10ZW1wbGF0ZS1wbnBtJyxcbn0gYXMgY29uc3RcblxuYXN5bmMgZnVuY3Rpb24gY2hlY2tHaXRDb21tYW5kKCk6IFByb21pc2U8Ym9vbGVhbj4ge1xuICB0cnkge1xuICAgIGF3YWl0IG5ldyBQcm9taXNlKChyZXNvbHZlKSA9PiB7XG4gICAgICBjb25zdCBjcCA9IGV4ZWMoJ2dpdCAtLXZlcnNpb24nKVxuICAgICAgY3Aub24oJ2Vycm9yJywgKCkgPT4ge1xuICAgICAgICByZXNvbHZlKGZhbHNlKVxuICAgICAgfSlcbiAgICAgIGNwLm9uKCdleGl0JywgKGNvZGUpID0+IHtcbiAgICAgICAgaWYgKGNvZGUgPT09IDApIHtcbiAgICAgICAgICByZXNvbHZlKHRydWUpXG4gICAgICAgIH0gZWxzZSB7XG4gICAgICAgICAgcmVzb2x2ZShmYWxzZSlcbiAgICAgICAgfVxuICAgICAgfSlcbiAgICB9KVxuICAgIHJldHVybiB0cnVlXG4gIH0gY2F0Y2gge1xuICAgIHJldHVybiBmYWxzZVxuICB9XG59XG5cbmFzeW5jIGZ1bmN0aW9uIGVuc3VyZUNhY2hlRGlyKFxuICBwYWNrYWdlTWFuYWdlcjogU3VwcG9ydGVkUGFja2FnZU1hbmFnZXIsXG4pOiBQcm9taXNlPHN0cmluZz4ge1xuICBjb25zdCBjYWNoZURpciA9IHBhdGguam9pbihob21lZGlyKCksICcubmFwaS1ycycsICd0ZW1wbGF0ZScsIHBhY2thZ2VNYW5hZ2VyKVxuICBhd2FpdCBta2RpckFzeW5jKGNhY2hlRGlyLCB7IHJlY3Vyc2l2ZTogdHJ1ZSB9KVxuICByZXR1cm4gY2FjaGVEaXJcbn1cblxuYXN5bmMgZnVuY3Rpb24gZG93bmxvYWRUZW1wbGF0ZShcbiAgcGFja2FnZU1hbmFnZXI6IFN1cHBvcnRlZFBhY2thZ2VNYW5hZ2VyLFxuICBjYWNoZURpcjogc3RyaW5nLFxuKTogUHJvbWlzZTx2b2lkPiB7XG4gIGNvbnN0IHJlcG9VcmwgPSBURU1QTEFURV9SRVBPU1twYWNrYWdlTWFuYWdlcl1cbiAgY29uc3QgdGVtcGxhdGVQYXRoID0gcGF0aC5qb2luKGNhY2hlRGlyLCAncmVwbycpXG5cbiAgaWYgKGV4aXN0c1N5bmModGVtcGxhdGVQYXRoKSkge1xuICAgIGRlYnVnKGBUZW1wbGF0ZSBjYWNoZSBmb3VuZCBhdCAke3RlbXBsYXRlUGF0aH0sIHVwZGF0aW5nLi4uYClcbiAgICB0cnkge1xuICAgICAgLy8gRmV0Y2ggbGF0ZXN0IGNoYW5nZXMgYW5kIHJlc2V0IHRvIHJlbW90ZVxuICAgICAgYXdhaXQgbmV3IFByb21pc2U8dm9pZD4oKHJlc29sdmUsIHJlamVjdCkgPT4ge1xuICAgICAgICBjb25zdCBjcCA9IGV4ZWMoJ2dpdCBmZXRjaCBvcmlnaW4nLCB7IGN3ZDogdGVtcGxhdGVQYXRoIH0pXG4gICAgICAgIGNwLm9uKCdlcnJvcicsIHJlamVjdClcbiAgICAgICAgY3Aub24oJ2V4aXQnLCAoY29kZSkgPT4ge1xuICAgICAgICAgIGlmIChjb2RlID09PSAwKSB7XG4gICAgICAgICAgICByZXNvbHZlKClcbiAgICAgICAgICB9IGVsc2Uge1xuICAgICAgICAgICAgcmVqZWN0KFxuICAgICAgICAgICAgICBuZXcgRXJyb3IoXG4gICAgICAgICAgICAgICAgYEZhaWxlZCB0byBmZXRjaCBsYXRlc3QgY2hhbmdlcywgZ2l0IHByb2Nlc3MgZXhpdGVkIHdpdGggY29kZSAke2NvZGV9YCxcbiAgICAgICAgICAgICAgKSxcbiAgICAgICAgICAgIClcbiAgICAgICAgICB9XG4gICAgICAgIH0pXG4gICAgICB9KVxuICAgICAgZXhlY1N5bmMoJ2dpdCByZXNldCAtLWhhcmQgb3JpZ2luL21haW4nLCB7XG4gICAgICAgIGN3ZDogdGVtcGxhdGVQYXRoLFxuICAgICAgICBzdGRpbzogJ2lnbm9yZScsXG4gICAgICB9KVxuICAgICAgZGVidWcoJ1RlbXBsYXRlIHVwZGF0ZWQgc3VjY2Vzc2Z1bGx5JylcbiAgICB9IGNhdGNoIChlcnJvcikge1xuICAgICAgZGVidWcoYEZhaWxlZCB0byB1cGRhdGUgdGVtcGxhdGU6ICR7ZXJyb3J9YClcbiAgICAgIHRocm93IG5ldyBFcnJvcihgRmFpbGVkIHRvIHVwZGF0ZSB0ZW1wbGF0ZSBmcm9tICR7cmVwb1VybH06ICR7ZXJyb3J9YClcbiAgICB9XG4gIH0gZWxzZSB7XG4gICAgZGVidWcoYENsb25pbmcgdGVtcGxhdGUgZnJvbSAke3JlcG9Vcmx9Li4uYClcbiAgICB0cnkge1xuICAgICAgZXhlY1N5bmMoYGdpdCBjbG9uZSAke3JlcG9Vcmx9IHJlcG9gLCB7IGN3ZDogY2FjaGVEaXIsIHN0ZGlvOiAnaW5oZXJpdCcgfSlcbiAgICAgIGRlYnVnKCdUZW1wbGF0ZSBjbG9uZWQgc3VjY2Vzc2Z1bGx5JylcbiAgICB9IGNhdGNoIChlcnJvcikge1xuICAgICAgdGhyb3cgbmV3IEVycm9yKGBGYWlsZWQgdG8gY2xvbmUgdGVtcGxhdGUgZnJvbSAke3JlcG9Vcmx9OiAke2Vycm9yfWApXG4gICAgfVxuICB9XG59XG5cbmFzeW5jIGZ1bmN0aW9uIGNvcHlEaXJlY3RvcnkoXG4gIHNyYzogc3RyaW5nLFxuICBkZXN0OiBzdHJpbmcsXG4gIGluY2x1ZGVXYXNpQmluZGluZ3M6IGJvb2xlYW4sXG4pOiBQcm9taXNlPHZvaWQ+IHtcbiAgYXdhaXQgbWtkaXJBc3luYyhkZXN0LCB7IHJlY3Vyc2l2ZTogdHJ1ZSB9KVxuICBjb25zdCBlbnRyaWVzID0gYXdhaXQgZnMucmVhZGRpcihzcmMsIHsgd2l0aEZpbGVUeXBlczogdHJ1ZSB9KVxuXG4gIGZvciAoY29uc3QgZW50cnkgb2YgZW50cmllcykge1xuICAgIGNvbnN0IHNyY1BhdGggPSBwYXRoLmpvaW4oc3JjLCBlbnRyeS5uYW1lKVxuICAgIGNvbnN0IGRlc3RQYXRoID0gcGF0aC5qb2luKGRlc3QsIGVudHJ5Lm5hbWUpXG5cbiAgICAvLyBTa2lwIC5naXQgZGlyZWN0b3J5XG4gICAgaWYgKGVudHJ5Lm5hbWUgPT09ICcuZ2l0Jykge1xuICAgICAgY29udGludWVcbiAgICB9XG5cbiAgICBpZiAoZW50cnkuaXNEaXJlY3RvcnkoKSkge1xuICAgICAgYXdhaXQgY29weURpcmVjdG9yeShzcmNQYXRoLCBkZXN0UGF0aCwgaW5jbHVkZVdhc2lCaW5kaW5ncylcbiAgICB9IGVsc2Uge1xuICAgICAgaWYgKFxuICAgICAgICAhaW5jbHVkZVdhc2lCaW5kaW5ncyAmJlxuICAgICAgICAoZW50cnkubmFtZS5lbmRzV2l0aCgnLndhc2ktYnJvd3Nlci5qcycpIHx8XG4gICAgICAgICAgZW50cnkubmFtZS5lbmRzV2l0aCgnLndhc2kuY2pzJykgfHxcbiAgICAgICAgICBlbnRyeS5uYW1lLmVuZHNXaXRoKCd3YXNpLXdvcmtlci5icm93c2VyLm1qcyAnKSB8fFxuICAgICAgICAgIGVudHJ5Lm5hbWUuZW5kc1dpdGgoJ3dhc2ktd29ya2VyLm1qcycpIHx8XG4gICAgICAgICAgZW50cnkubmFtZS5lbmRzV2l0aCgnYnJvd3Nlci5qcycpKVxuICAgICAgKSB7XG4gICAgICAgIGNvbnRpbnVlXG4gICAgICB9XG4gICAgICBhd2FpdCBmcy5jb3B5RmlsZShzcmNQYXRoLCBkZXN0UGF0aClcbiAgICB9XG4gIH1cbn1cblxuYXN5bmMgZnVuY3Rpb24gZmlsdGVyVGFyZ2V0c0luUGFja2FnZUpzb24oXG4gIGZpbGVQYXRoOiBzdHJpbmcsXG4gIGVuYWJsZWRUYXJnZXRzOiBzdHJpbmdbXSxcbik6IFByb21pc2U8dm9pZD4ge1xuICBjb25zdCBjb250ZW50ID0gYXdhaXQgZnMucmVhZEZpbGUoZmlsZVBhdGgsICd1dGYtOCcpXG4gIGNvbnN0IHBhY2thZ2VKc29uID0gSlNPTi5wYXJzZShjb250ZW50KVxuXG4gIC8vIEZpbHRlciBuYXBpLnRhcmdldHNcbiAgaWYgKHBhY2thZ2VKc29uLm5hcGk/LnRhcmdldHMpIHtcbiAgICBwYWNrYWdlSnNvbi5uYXBpLnRhcmdldHMgPSBwYWNrYWdlSnNvbi5uYXBpLnRhcmdldHMuZmlsdGVyKFxuICAgICAgKHRhcmdldDogc3RyaW5nKSA9PiBlbmFibGVkVGFyZ2V0cy5pbmNsdWRlcyh0YXJnZXQpLFxuICAgIClcbiAgfVxuXG4gIGF3YWl0IGZzLndyaXRlRmlsZShmaWxlUGF0aCwgSlNPTi5zdHJpbmdpZnkocGFja2FnZUpzb24sIG51bGwsIDIpICsgJ1xcbicpXG59XG5cbmFzeW5jIGZ1bmN0aW9uIGZpbHRlclRhcmdldHNJbkdpdGh1YkFjdGlvbnMoXG4gIGZpbGVQYXRoOiBzdHJpbmcsXG4gIGVuYWJsZWRUYXJnZXRzOiBzdHJpbmdbXSxcbik6IFByb21pc2U8dm9pZD4ge1xuICBjb25zdCBjb250ZW50ID0gYXdhaXQgZnMucmVhZEZpbGUoZmlsZVBhdGgsICd1dGYtOCcpXG4gIGNvbnN0IHlhbWwgPSB5YW1sTG9hZChjb250ZW50KSBhcyBhbnlcblxuICBjb25zdCBtYWNPU0FuZFdpbmRvd3NUYXJnZXRzID0gbmV3IFNldChbXG4gICAgJ3g4Nl82NC1wYy13aW5kb3dzLW1zdmMnLFxuICAgICd4ODZfNjQtcGMtd2luZG93cy1nbnUnLFxuICAgICdhYXJjaDY0LXBjLXdpbmRvd3MtbXN2YycsXG4gICAgJ3g4Nl82NC1hcHBsZS1kYXJ3aW4nLFxuICBdKVxuXG4gIGNvbnN0IGxpbnV4VGFyZ2V0cyA9IG5ldyBTZXQoW1xuICAgICd4ODZfNjQtdW5rbm93bi1saW51eC1nbnUnLFxuICAgICd4ODZfNjQtdW5rbm93bi1saW51eC1tdXNsJyxcbiAgICAnYWFyY2g2NC11bmtub3duLWxpbnV4LWdudScsXG4gICAgJ2FhcmNoNjQtdW5rbm93bi1saW51eC1tdXNsJyxcbiAgICAnYXJtdjctdW5rbm93bi1saW51eC1nbnVlYWJpaGYnLFxuICAgICdhcm12Ny11bmtub3duLWxpbnV4LW11c2xlYWJpaGYnLFxuICAgICdsb29uZ2FyY2g2NC11bmtub3duLWxpbnV4LWdudScsXG4gICAgJ3Jpc2N2NjRnYy11bmtub3duLWxpbnV4LWdudScsXG4gICAgJ3Bvd2VycGM2NGxlLXVua25vd24tbGludXgtZ251JyxcbiAgICAnczM5MHgtdW5rbm93bi1saW51eC1nbnUnLFxuICAgICdhYXJjaDY0LWxpbnV4LWFuZHJvaWQnLFxuICAgICdhcm12Ny1saW51eC1hbmRyb2lkZWFiaScsXG4gIF0pXG5cbiAgLy8gQ2hlY2sgaWYgYW55IExpbnV4IHRhcmdldHMgYXJlIGVuYWJsZWRcbiAgY29uc3QgaGFzTGludXhUYXJnZXRzID0gZW5hYmxlZFRhcmdldHMuc29tZSgodGFyZ2V0KSA9PlxuICAgIGxpbnV4VGFyZ2V0cy5oYXModGFyZ2V0KSxcbiAgKVxuXG4gIC8vIEZpbHRlciB0aGUgbWF0cml4IGNvbmZpZ3VyYXRpb25zIGluIHRoZSBidWlsZCBqb2JcbiAgaWYgKHlhbWw/LmpvYnM/LmJ1aWxkPy5zdHJhdGVneT8ubWF0cml4Py5zZXR0aW5ncykge1xuICAgIHlhbWwuam9icy5idWlsZC5zdHJhdGVneS5tYXRyaXguc2V0dGluZ3MgPVxuICAgICAgeWFtbC5qb2JzLmJ1aWxkLnN0cmF0ZWd5Lm1hdHJpeC5zZXR0aW5ncy5maWx0ZXIoKHNldHRpbmc6IGFueSkgPT4ge1xuICAgICAgICBpZiAoc2V0dGluZy50YXJnZXQpIHtcbiAgICAgICAgICByZXR1cm4gZW5hYmxlZFRhcmdldHMuaW5jbHVkZXMoc2V0dGluZy50YXJnZXQpXG4gICAgICAgIH1cbiAgICAgICAgcmV0dXJuIHRydWVcbiAgICAgIH0pXG4gIH1cblxuICBjb25zdCBqb2JzVG9SZW1vdmU6IHN0cmluZ1tdID0gW11cblxuICBpZiAoZW5hYmxlZFRhcmdldHMuZXZlcnkoKHRhcmdldCkgPT4gIW1hY09TQW5kV2luZG93c1RhcmdldHMuaGFzKHRhcmdldCkpKSB7XG4gICAgam9ic1RvUmVtb3ZlLnB1c2goJ3Rlc3QtbWFjT1Mtd2luZG93cy1iaW5kaW5nJylcbiAgfSBlbHNlIHtcbiAgICAvLyBGaWx0ZXIgdGhlIG1hdHJpeCBjb25maWd1cmF0aW9ucyBpbiB0aGUgdGVzdC1tYWNPUy13aW5kb3dzLWJpbmRpbmcgam9iXG4gICAgaWYgKFxuICAgICAgeWFtbD8uam9icz8uWyd0ZXN0LW1hY09TLXdpbmRvd3MtYmluZGluZyddPy5zdHJhdGVneT8ubWF0cml4Py5zZXR0aW5nc1xuICAgICkge1xuICAgICAgeWFtbC5qb2JzWyd0ZXN0LW1hY09TLXdpbmRvd3MtYmluZGluZyddLnN0cmF0ZWd5Lm1hdHJpeC5zZXR0aW5ncyA9XG4gICAgICAgIHlhbWwuam9ic1sndGVzdC1tYWNPUy13aW5kb3dzLWJpbmRpbmcnXS5zdHJhdGVneS5tYXRyaXguc2V0dGluZ3MuZmlsdGVyKFxuICAgICAgICAgIChzZXR0aW5nOiBhbnkpID0+IHtcbiAgICAgICAgICAgIGlmIChzZXR0aW5nLnRhcmdldCkge1xuICAgICAgICAgICAgICByZXR1cm4gZW5hYmxlZFRhcmdldHMuaW5jbHVkZXMoc2V0dGluZy50YXJnZXQpXG4gICAgICAgICAgICB9XG4gICAgICAgICAgICByZXR1cm4gdHJ1ZVxuICAgICAgICAgIH0sXG4gICAgICAgIClcbiAgICB9XG4gIH1cblxuICAvLyBJZiBubyBMaW51eCB0YXJnZXRzIGFyZSBlbmFibGVkLCByZW1vdmUgTGludXgtc3BlY2lmaWMgam9ic1xuICBpZiAoIWhhc0xpbnV4VGFyZ2V0cykge1xuICAgIC8vIFJlbW92ZSB0ZXN0LWxpbnV4LWJpbmRpbmcgam9iXG4gICAgaWYgKHlhbWw/LmpvYnM/LlsndGVzdC1saW51eC1iaW5kaW5nJ10pIHtcbiAgICAgIGpvYnNUb1JlbW92ZS5wdXNoKCd0ZXN0LWxpbnV4LWJpbmRpbmcnKVxuICAgIH1cbiAgfSBlbHNlIHtcbiAgICAvLyBGaWx0ZXIgdGhlIG1hdHJpeCBjb25maWd1cmF0aW9ucyBpbiB0aGUgdGVzdC1saW51eC14NjQtZ251LWJpbmRpbmcgam9iXG4gICAgaWYgKHlhbWw/LmpvYnM/LlsndGVzdC1saW51eC1iaW5kaW5nJ10/LnN0cmF0ZWd5Py5tYXRyaXg/LnRhcmdldCkge1xuICAgICAgeWFtbC5qb2JzWyd0ZXN0LWxpbnV4LWJpbmRpbmcnXS5zdHJhdGVneS5tYXRyaXgudGFyZ2V0ID0geWFtbC5qb2JzW1xuICAgICAgICAndGVzdC1saW51eC1iaW5kaW5nJ1xuICAgICAgXS5zdHJhdGVneS5tYXRyaXgudGFyZ2V0LmZpbHRlcigodGFyZ2V0OiBzdHJpbmcpID0+IHtcbiAgICAgICAgaWYgKHRhcmdldCkge1xuICAgICAgICAgIHJldHVybiBlbmFibGVkVGFyZ2V0cy5pbmNsdWRlcyh0YXJnZXQpXG4gICAgICAgIH1cbiAgICAgICAgcmV0dXJuIHRydWVcbiAgICAgIH0pXG4gICAgfVxuICB9XG5cbiAgaWYgKCFlbmFibGVkVGFyZ2V0cy5pbmNsdWRlcygnd2FzbTMyLXdhc2lwMS10aHJlYWRzJykpIHtcbiAgICBqb2JzVG9SZW1vdmUucHVzaCgndGVzdC13YXNpJylcbiAgfVxuXG4gIGlmICghZW5hYmxlZFRhcmdldHMuaW5jbHVkZXMoJ3g4Nl82NC11bmtub3duLWZyZWVic2QnKSkge1xuICAgIGpvYnNUb1JlbW92ZS5wdXNoKCdidWlsZC1mcmVlYnNkJylcbiAgfVxuXG4gIC8vIEZpbHRlciBvdGhlciB0ZXN0IGpvYnMgYmFzZWQgb24gdGFyZ2V0XG4gIGZvciAoY29uc3QgW2pvYk5hbWUsIGpvYkNvbmZpZ10gb2YgT2JqZWN0LmVudHJpZXMoeWFtbC5qb2JzIHx8IHt9KSkge1xuICAgIGlmIChcbiAgICAgIGpvYk5hbWUuc3RhcnRzV2l0aCgndGVzdC0nKSAmJlxuICAgICAgam9iTmFtZSAhPT0gJ3Rlc3QtbWFjT1Mtd2luZG93cy1iaW5kaW5nJyAmJlxuICAgICAgam9iTmFtZSAhPT0gJ3Rlc3QtbGludXgteDY0LWdudS1iaW5kaW5nJ1xuICAgICkge1xuICAgICAgLy8gRXh0cmFjdCB0YXJnZXQgZnJvbSBqb2IgbmFtZSBvciBjb25maWdcbiAgICAgIGNvbnN0IGpvYiA9IGpvYkNvbmZpZyBhcyBhbnlcbiAgICAgIGlmIChqb2Iuc3RyYXRlZ3k/Lm1hdHJpeD8uc2V0dGluZ3M/LlswXT8udGFyZ2V0KSB7XG4gICAgICAgIGNvbnN0IHRhcmdldCA9IGpvYi5zdHJhdGVneS5tYXRyaXguc2V0dGluZ3NbMF0udGFyZ2V0XG4gICAgICAgIGlmICghZW5hYmxlZFRhcmdldHMuaW5jbHVkZXModGFyZ2V0KSkge1xuICAgICAgICAgIGpvYnNUb1JlbW92ZS5wdXNoKGpvYk5hbWUpXG4gICAgICAgIH1cbiAgICAgIH1cbiAgICB9XG4gIH1cblxuICAvLyBSZW1vdmUgam9icyBmb3IgZGlzYWJsZWQgdGFyZ2V0c1xuICBmb3IgKGNvbnN0IGpvYk5hbWUgb2Ygam9ic1RvUmVtb3ZlKSB7XG4gICAgZGVsZXRlIHlhbWwuam9ic1tqb2JOYW1lXVxuICB9XG5cbiAgaWYgKEFycmF5LmlzQXJyYXkoeWFtbC5qb2JzPy5wdWJsaXNoPy5uZWVkcykpIHtcbiAgICB5YW1sLmpvYnMucHVibGlzaC5uZWVkcyA9IHlhbWwuam9icy5wdWJsaXNoLm5lZWRzLmZpbHRlcihcbiAgICAgIChuZWVkOiBzdHJpbmcpID0+ICFqb2JzVG9SZW1vdmUuaW5jbHVkZXMobmVlZCksXG4gICAgKVxuICB9XG5cbiAgLy8gV3JpdGUgYmFjayB0aGUgZmlsdGVyZWQgWUFNTFxuICBjb25zdCB1cGRhdGVkWWFtbCA9IHlhbWxEdW1wKHlhbWwsIHtcbiAgICBsaW5lV2lkdGg6IC0xLFxuICAgIG5vUmVmczogdHJ1ZSxcbiAgICBzb3J0S2V5czogZmFsc2UsXG4gIH0pXG4gIGF3YWl0IGZzLndyaXRlRmlsZShmaWxlUGF0aCwgdXBkYXRlZFlhbWwpXG59XG5cbmZ1bmN0aW9uIHByb2Nlc3NPcHRpb25zKG9wdGlvbnM6IFJhd05ld09wdGlvbnMpIHtcbiAgZGVidWcoJ1Byb2Nlc3Npbmcgb3B0aW9ucy4uLicpXG4gIGlmICghb3B0aW9ucy5wYXRoKSB7XG4gICAgdGhyb3cgbmV3IEVycm9yKCdQbGVhc2UgcHJvdmlkZSB0aGUgcGF0aCBhcyB0aGUgYXJndW1lbnQnKVxuICB9XG4gIG9wdGlvbnMucGF0aCA9IHBhdGgucmVzb2x2ZShwcm9jZXNzLmN3ZCgpLCBvcHRpb25zLnBhdGgpXG4gIGRlYnVnKGBSZXNvbHZlZCB0YXJnZXQgcGF0aCB0bzogJHtvcHRpb25zLnBhdGh9YClcblxuICBpZiAoIW9wdGlvbnMubmFtZSkge1xuICAgIG9wdGlvbnMubmFtZSA9IHBhdGgucGFyc2Uob3B0aW9ucy5wYXRoKS5iYXNlXG4gICAgZGVidWcoYE5vIHByb2plY3QgbmFtZSBwcm92aWRlZCwgZml4IGl0IHRvIGRpciBuYW1lOiAke29wdGlvbnMubmFtZX1gKVxuICB9XG5cbiAgaWYgKCFvcHRpb25zLnRhcmdldHM/Lmxlbmd0aCkge1xuICAgIGlmIChvcHRpb25zLmVuYWJsZUFsbFRhcmdldHMpIHtcbiAgICAgIG9wdGlvbnMudGFyZ2V0cyA9IEFWQUlMQUJMRV9UQVJHRVRTLmNvbmNhdCgpXG4gICAgICBkZWJ1ZygnRW5hYmxlIGFsbCB0YXJnZXRzJylcbiAgICB9IGVsc2UgaWYgKG9wdGlvbnMuZW5hYmxlRGVmYXVsdFRhcmdldHMpIHtcbiAgICAgIG9wdGlvbnMudGFyZ2V0cyA9IERFRkFVTFRfVEFSR0VUUy5jb25jYXQoKVxuICAgICAgZGVidWcoJ0VuYWJsZSBkZWZhdWx0IHRhcmdldHMnKVxuICAgIH0gZWxzZSB7XG4gICAgICB0aHJvdyBuZXcgRXJyb3IoJ0F0IGxlYXN0IG9uZSB0YXJnZXQgbXVzdCBiZSBlbmFibGVkJylcbiAgICB9XG4gIH1cbiAgaWYgKFxuICAgIG9wdGlvbnMudGFyZ2V0cy5zb21lKCh0YXJnZXQpID0+IHRhcmdldCA9PT0gJ3dhc20zMi13YXNpLXByZXZpZXcxLXRocmVhZHMnKVxuICApIHtcbiAgICBjb25zdCBvdXQgPSBleGVjU3luYyhgcnVzdHVwIHRhcmdldCBsaXN0YCwge1xuICAgICAgZW5jb2Rpbmc6ICd1dGY4JyxcbiAgICB9KVxuICAgIGlmIChvdXQuaW5jbHVkZXMoJ3dhc20zMi13YXNpcDEtdGhyZWFkcycpKSB7XG4gICAgICBvcHRpb25zLnRhcmdldHMgPSBvcHRpb25zLnRhcmdldHMubWFwKCh0YXJnZXQpID0+XG4gICAgICAgIHRhcmdldCA9PT0gJ3dhc20zMi13YXNpLXByZXZpZXcxLXRocmVhZHMnXG4gICAgICAgICAgPyAnd2FzbTMyLXdhc2lwMS10aHJlYWRzJ1xuICAgICAgICAgIDogdGFyZ2V0LFxuICAgICAgKVxuICAgIH1cbiAgfVxuXG4gIHJldHVybiBhcHBseURlZmF1bHROZXdPcHRpb25zKG9wdGlvbnMpIGFzIE5ld09wdGlvbnNcbn1cblxuZXhwb3J0IGFzeW5jIGZ1bmN0aW9uIG5ld1Byb2plY3QodXNlck9wdGlvbnM6IFJhd05ld09wdGlvbnMpIHtcbiAgZGVidWcoJ1dpbGwgY3JlYXRlIG5hcGktcnMgcHJvamVjdCB3aXRoIGdpdmVuIG9wdGlvbnM6JylcbiAgZGVidWcodXNlck9wdGlvbnMpXG5cbiAgY29uc3Qgb3B0aW9ucyA9IHByb2Nlc3NPcHRpb25zKHVzZXJPcHRpb25zKVxuXG4gIGRlYnVnKCdUYXJnZXRzIHRvIGJlIGVuYWJsZWQ6JylcbiAgZGVidWcob3B0aW9ucy50YXJnZXRzKVxuXG4gIC8vIENoZWNrIGlmIGdpdCBpcyBhdmFpbGFibGVcbiAgaWYgKCEoYXdhaXQgY2hlY2tHaXRDb21tYW5kKCkpKSB7XG4gICAgdGhyb3cgbmV3IEVycm9yKFxuICAgICAgJ0dpdCBpcyBub3QgaW5zdGFsbGVkIG9yIG5vdCBhdmFpbGFibGUgaW4gUEFUSC4gUGxlYXNlIGluc3RhbGwgR2l0IHRvIGNvbnRpbnVlLicsXG4gICAgKVxuICB9XG5cbiAgY29uc3QgcGFja2FnZU1hbmFnZXIgPSBvcHRpb25zLnBhY2thZ2VNYW5hZ2VyIGFzIFN1cHBvcnRlZFBhY2thZ2VNYW5hZ2VyXG5cbiAgLy8gRW5zdXJlIHRhcmdldCBkaXJlY3RvcnkgZXhpc3RzIGFuZCBpcyBlbXB0eVxuICBhd2FpdCBlbnN1cmVQYXRoKG9wdGlvbnMucGF0aCwgb3B0aW9ucy5kcnlSdW4pXG5cbiAgaWYgKCFvcHRpb25zLmRyeVJ1bikge1xuICAgIHRyeSB7XG4gICAgICAvLyBEb3dubG9hZCBvciB1cGRhdGUgdGVtcGxhdGVcbiAgICAgIGNvbnN0IGNhY2hlRGlyID0gYXdhaXQgZW5zdXJlQ2FjaGVEaXIocGFja2FnZU1hbmFnZXIpXG4gICAgICBhd2FpdCBkb3dubG9hZFRlbXBsYXRlKHBhY2thZ2VNYW5hZ2VyLCBjYWNoZURpcilcblxuICAgICAgLy8gQ29weSB0ZW1wbGF0ZSBmaWxlcyB0byB0YXJnZXQgZGlyZWN0b3J5XG4gICAgICBjb25zdCB0ZW1wbGF0ZVBhdGggPSBwYXRoLmpvaW4oY2FjaGVEaXIsICdyZXBvJylcbiAgICAgIGF3YWl0IGNvcHlEaXJlY3RvcnkoXG4gICAgICAgIHRlbXBsYXRlUGF0aCxcbiAgICAgICAgb3B0aW9ucy5wYXRoLFxuICAgICAgICBvcHRpb25zLnRhcmdldHMuaW5jbHVkZXMoJ3dhc20zMi13YXNpcDEtdGhyZWFkcycpLFxuICAgICAgKVxuXG4gICAgICAvLyBSZW5hbWUgcHJvamVjdCB1c2luZyB0aGUgcmVuYW1lIEFQSVxuICAgICAgYXdhaXQgcmVuYW1lUHJvamVjdCh7XG4gICAgICAgIGN3ZDogb3B0aW9ucy5wYXRoLFxuICAgICAgICBuYW1lOiBvcHRpb25zLm5hbWUsXG4gICAgICAgIGJpbmFyeU5hbWU6IGdldEJpbmFyeU5hbWUob3B0aW9ucy5uYW1lKSxcbiAgICAgIH0pXG5cbiAgICAgIC8vIEZpbHRlciB0YXJnZXRzIGluIHBhY2thZ2UuanNvblxuICAgICAgY29uc3QgcGFja2FnZUpzb25QYXRoID0gcGF0aC5qb2luKG9wdGlvbnMucGF0aCwgJ3BhY2thZ2UuanNvbicpXG4gICAgICBpZiAoZXhpc3RzU3luYyhwYWNrYWdlSnNvblBhdGgpKSB7XG4gICAgICAgIGF3YWl0IGZpbHRlclRhcmdldHNJblBhY2thZ2VKc29uKHBhY2thZ2VKc29uUGF0aCwgb3B0aW9ucy50YXJnZXRzKVxuICAgICAgfVxuXG4gICAgICAvLyBGaWx0ZXIgdGFyZ2V0cyBpbiBHaXRIdWIgQWN0aW9ucyBDSVxuICAgICAgY29uc3QgY2lQYXRoID0gcGF0aC5qb2luKG9wdGlvbnMucGF0aCwgJy5naXRodWInLCAnd29ya2Zsb3dzJywgJ0NJLnltbCcpXG4gICAgICBpZiAoZXhpc3RzU3luYyhjaVBhdGgpICYmIG9wdGlvbnMuZW5hYmxlR2l0aHViQWN0aW9ucykge1xuICAgICAgICBhd2FpdCBmaWx0ZXJUYXJnZXRzSW5HaXRodWJBY3Rpb25zKGNpUGF0aCwgb3B0aW9ucy50YXJnZXRzKVxuICAgICAgfSBlbHNlIGlmIChcbiAgICAgICAgIW9wdGlvbnMuZW5hYmxlR2l0aHViQWN0aW9ucyAmJlxuICAgICAgICBleGlzdHNTeW5jKHBhdGguam9pbihvcHRpb25zLnBhdGgsICcuZ2l0aHViJykpXG4gICAgICApIHtcbiAgICAgICAgLy8gUmVtb3ZlIC5naXRodWIgZGlyZWN0b3J5IGlmIEdpdEh1YiBBY3Rpb25zIGlzIG5vdCBlbmFibGVkXG4gICAgICAgIGF3YWl0IGZzLnJtKHBhdGguam9pbihvcHRpb25zLnBhdGgsICcuZ2l0aHViJyksIHtcbiAgICAgICAgICByZWN1cnNpdmU6IHRydWUsXG4gICAgICAgICAgZm9yY2U6IHRydWUsXG4gICAgICAgIH0pXG4gICAgICB9XG5cbiAgICAgIC8vIFVwZGF0ZSBwYWNrYWdlLmpzb24gd2l0aCBhZGRpdGlvbmFsIGNvbmZpZ3VyYXRpb25zXG4gICAgICBjb25zdCBwa2dKc29uQ29udGVudCA9IGF3YWl0IGZzLnJlYWRGaWxlKHBhY2thZ2VKc29uUGF0aCwgJ3V0Zi04JylcbiAgICAgIGNvbnN0IHBrZ0pzb24gPSBKU09OLnBhcnNlKHBrZ0pzb25Db250ZW50KVxuXG4gICAgICAvLyBVcGRhdGUgZW5naW5lIHJlcXVpcmVtZW50XG4gICAgICBpZiAoIXBrZ0pzb24uZW5naW5lcykge1xuICAgICAgICBwa2dKc29uLmVuZ2luZXMgPSB7fVxuICAgICAgfVxuICAgICAgcGtnSnNvbi5lbmdpbmVzLm5vZGUgPSBuYXBpRW5naW5lUmVxdWlyZW1lbnQob3B0aW9ucy5taW5Ob2RlQXBpVmVyc2lvbilcblxuICAgICAgLy8gVXBkYXRlIGxpY2Vuc2UgaWYgZGlmZmVyZW50IGZyb20gdGVtcGxhdGVcbiAgICAgIGlmIChvcHRpb25zLmxpY2Vuc2UgJiYgcGtnSnNvbi5saWNlbnNlICE9PSBvcHRpb25zLmxpY2Vuc2UpIHtcbiAgICAgICAgcGtnSnNvbi5saWNlbnNlID0gb3B0aW9ucy5saWNlbnNlXG4gICAgICB9XG5cbiAgICAgIC8vIFVwZGF0ZSB0ZXN0IGZyYW1ld29yayBpZiBuZWVkZWRcbiAgICAgIGlmIChvcHRpb25zLnRlc3RGcmFtZXdvcmsgIT09ICdhdmEnKSB7XG4gICAgICAgIC8vIFRoaXMgd291bGQgcmVxdWlyZSBtb3JlIGNvbXBsZXggbG9naWMgdG8gdXBkYXRlIHRlc3Qgc2NyaXB0cyBhbmQgZGVwZW5kZW5jaWVzXG4gICAgICAgIGRlYnVnKFxuICAgICAgICAgIGBUZXN0IGZyYW1ld29yayAke29wdGlvbnMudGVzdEZyYW1ld29ya30gcmVxdWVzdGVkIGJ1dCBub3QgeWV0IGltcGxlbWVudGVkYCxcbiAgICAgICAgKVxuICAgICAgfVxuXG4gICAgICBhd2FpdCBmcy53cml0ZUZpbGUoXG4gICAgICAgIHBhY2thZ2VKc29uUGF0aCxcbiAgICAgICAgSlNPTi5zdHJpbmdpZnkocGtnSnNvbiwgbnVsbCwgMikgKyAnXFxuJyxcbiAgICAgIClcbiAgICB9IGNhdGNoIChlcnJvcikge1xuICAgICAgdGhyb3cgbmV3IEVycm9yKGBGYWlsZWQgdG8gY3JlYXRlIHByb2plY3Q6ICR7ZXJyb3J9YClcbiAgICB9XG4gIH1cblxuICBkZWJ1ZyhgUHJvamVjdCBjcmVhdGVkIGF0OiAke29wdGlvbnMucGF0aH1gKVxufVxuXG5hc3luYyBmdW5jdGlvbiBlbnN1cmVQYXRoKHBhdGg6IHN0cmluZywgZHJ5UnVuID0gZmFsc2UpIHtcbiAgY29uc3Qgc3RhdCA9IGF3YWl0IHN0YXRBc3luYyhwYXRoLCB7fSkuY2F0Y2goKCkgPT4gdW5kZWZpbmVkKVxuXG4gIC8vIGZpbGUgZGVzY3JpcHRvciBleGlzdHNcbiAgaWYgKHN0YXQpIHtcbiAgICBpZiAoc3RhdC5pc0ZpbGUoKSkge1xuICAgICAgdGhyb3cgbmV3IEVycm9yKFxuICAgICAgICBgUGF0aCAke3BhdGh9IGZvciBjcmVhdGluZyBuZXcgbmFwaS1ycyBwcm9qZWN0IGFscmVhZHkgZXhpc3RzIGFuZCBpdCdzIG5vdCBhIGRpcmVjdG9yeS5gLFxuICAgICAgKVxuICAgIH0gZWxzZSBpZiAoc3RhdC5pc0RpcmVjdG9yeSgpKSB7XG4gICAgICBjb25zdCBmaWxlcyA9IGF3YWl0IHJlYWRkaXJBc3luYyhwYXRoKVxuICAgICAgaWYgKGZpbGVzLmxlbmd0aCkge1xuICAgICAgICB0aHJvdyBuZXcgRXJyb3IoXG4gICAgICAgICAgYFBhdGggJHtwYXRofSBmb3IgY3JlYXRpbmcgbmV3IG5hcGktcnMgcHJvamVjdCBhbHJlYWR5IGV4aXN0cyBhbmQgaXQncyBub3QgZW1wdHkuYCxcbiAgICAgICAgKVxuICAgICAgfVxuICAgIH1cbiAgfVxuXG4gIGlmICghZHJ5UnVuKSB7XG4gICAgdHJ5IHtcbiAgICAgIGRlYnVnKGBUcnkgdG8gY3JlYXRlIHRhcmdldCBkaXJlY3Rvcnk6ICR7cGF0aH1gKVxuICAgICAgaWYgKCFkcnlSdW4pIHtcbiAgICAgICAgYXdhaXQgbWtkaXJBc3luYyhwYXRoLCB7IHJlY3Vyc2l2ZTogdHJ1ZSB9KVxuICAgICAgfVxuICAgIH0gY2F0Y2ggKGUpIHtcbiAgICAgIHRocm93IG5ldyBFcnJvcihgRmFpbGVkIHRvIGNyZWF0ZSB0YXJnZXQgZGlyZWN0b3J5OiAke3BhdGh9YCwge1xuICAgICAgICBjYXVzZTogZSxcbiAgICAgIH0pXG4gICAgfVxuICB9XG59XG5cbmZ1bmN0aW9uIGdldEJpbmFyeU5hbWUobmFtZTogc3RyaW5nKTogc3RyaW5nIHtcbiAgcmV0dXJuIG5hbWUuc3BsaXQoJy8nKS5wb3AoKSFcbn1cblxuZXhwb3J0IHR5cGUgeyBOZXdPcHRpb25zIH1cbiIsIi8vIFRoaXMgZmlsZSBpcyBnZW5lcmF0ZWQgYnkgY29kZWdlbi9pbmRleC50c1xuLy8gRG8gbm90IGVkaXQgdGhpcyBmaWxlIG1hbnVhbGx5XG5pbXBvcnQgeyBDb21tYW5kLCBPcHRpb24gfSBmcm9tICdjbGlwYW5pb24nXG5cbmV4cG9ydCBhYnN0cmFjdCBjbGFzcyBCYXNlUHJlUHVibGlzaENvbW1hbmQgZXh0ZW5kcyBDb21tYW5kIHtcbiAgc3RhdGljIHBhdGhzID0gW1sncHJlLXB1Ymxpc2gnXSwgWydwcmVwdWJsaXNoJ11dXG5cbiAgc3RhdGljIHVzYWdlID0gQ29tbWFuZC5Vc2FnZSh7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnVXBkYXRlIHBhY2thZ2UuanNvbiBhbmQgY29weSBhZGRvbnMgaW50byBwZXIgcGxhdGZvcm0gcGFja2FnZXMnLFxuICB9KVxuXG4gIGN3ZCA9IE9wdGlvbi5TdHJpbmcoJy0tY3dkJywgcHJvY2Vzcy5jd2QoKSwge1xuICAgIGRlc2NyaXB0aW9uOlxuICAgICAgJ1RoZSB3b3JraW5nIGRpcmVjdG9yeSBvZiB3aGVyZSBuYXBpIGNvbW1hbmQgd2lsbCBiZSBleGVjdXRlZCBpbiwgYWxsIG90aGVyIHBhdGhzIG9wdGlvbnMgYXJlIHJlbGF0aXZlIHRvIHRoaXMgcGF0aCcsXG4gIH0pXG5cbiAgY29uZmlnUGF0aD86IHN0cmluZyA9IE9wdGlvbi5TdHJpbmcoJy0tY29uZmlnLXBhdGgsLWMnLCB7XG4gICAgZGVzY3JpcHRpb246ICdQYXRoIHRvIGBuYXBpYCBjb25maWcganNvbiBmaWxlJyxcbiAgfSlcblxuICBwYWNrYWdlSnNvblBhdGggPSBPcHRpb24uU3RyaW5nKCctLXBhY2thZ2UtanNvbi1wYXRoJywgJ3BhY2thZ2UuanNvbicsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1BhdGggdG8gYHBhY2thZ2UuanNvbmAnLFxuICB9KVxuXG4gIG5wbURpciA9IE9wdGlvbi5TdHJpbmcoJy0tbnBtLWRpciwtcCcsICducG0nLCB7XG4gICAgZGVzY3JpcHRpb246ICdQYXRoIHRvIHRoZSBmb2xkZXIgd2hlcmUgdGhlIG5wbSBwYWNrYWdlcyBwdXQnLFxuICB9KVxuXG4gIHRhZ1N0eWxlID0gT3B0aW9uLlN0cmluZygnLS10YWctc3R5bGUsLS10YWdzdHlsZSwtdCcsICdsZXJuYScsIHtcbiAgICBkZXNjcmlwdGlvbjogJ2dpdCB0YWcgc3R5bGUsIGBucG1gIG9yIGBsZXJuYWAnLFxuICB9KVxuXG4gIGdoUmVsZWFzZSA9IE9wdGlvbi5Cb29sZWFuKCctLWdoLXJlbGVhc2UnLCB0cnVlLCB7XG4gICAgZGVzY3JpcHRpb246ICdXaGV0aGVyIGNyZWF0ZSBHaXRIdWIgcmVsZWFzZScsXG4gIH0pXG5cbiAgZ2hSZWxlYXNlTmFtZT86IHN0cmluZyA9IE9wdGlvbi5TdHJpbmcoJy0tZ2gtcmVsZWFzZS1uYW1lJywge1xuICAgIGRlc2NyaXB0aW9uOiAnR2l0SHViIHJlbGVhc2UgbmFtZScsXG4gIH0pXG5cbiAgZ2hSZWxlYXNlSWQ/OiBzdHJpbmcgPSBPcHRpb24uU3RyaW5nKCctLWdoLXJlbGVhc2UtaWQnLCB7XG4gICAgZGVzY3JpcHRpb246ICdFeGlzdGluZyBHaXRIdWIgcmVsZWFzZSBpZCcsXG4gIH0pXG5cbiAgc2tpcE9wdGlvbmFsUHVibGlzaCA9IE9wdGlvbi5Cb29sZWFuKCctLXNraXAtb3B0aW9uYWwtcHVibGlzaCcsIGZhbHNlLCB7XG4gICAgZGVzY3JpcHRpb246ICdXaGV0aGVyIHNraXAgb3B0aW9uYWxEZXBlbmRlbmNpZXMgcGFja2FnZXMgcHVibGlzaCcsXG4gIH0pXG5cbiAgZHJ5UnVuID0gT3B0aW9uLkJvb2xlYW4oJy0tZHJ5LXJ1bicsIGZhbHNlLCB7XG4gICAgZGVzY3JpcHRpb246ICdEcnkgcnVuIHdpdGhvdXQgdG91Y2hpbmcgZmlsZSBzeXN0ZW0nLFxuICB9KVxuXG4gIGdldE9wdGlvbnMoKSB7XG4gICAgcmV0dXJuIHtcbiAgICAgIGN3ZDogdGhpcy5jd2QsXG4gICAgICBjb25maWdQYXRoOiB0aGlzLmNvbmZpZ1BhdGgsXG4gICAgICBwYWNrYWdlSnNvblBhdGg6IHRoaXMucGFja2FnZUpzb25QYXRoLFxuICAgICAgbnBtRGlyOiB0aGlzLm5wbURpcixcbiAgICAgIHRhZ1N0eWxlOiB0aGlzLnRhZ1N0eWxlLFxuICAgICAgZ2hSZWxlYXNlOiB0aGlzLmdoUmVsZWFzZSxcbiAgICAgIGdoUmVsZWFzZU5hbWU6IHRoaXMuZ2hSZWxlYXNlTmFtZSxcbiAgICAgIGdoUmVsZWFzZUlkOiB0aGlzLmdoUmVsZWFzZUlkLFxuICAgICAgc2tpcE9wdGlvbmFsUHVibGlzaDogdGhpcy5za2lwT3B0aW9uYWxQdWJsaXNoLFxuICAgICAgZHJ5UnVuOiB0aGlzLmRyeVJ1bixcbiAgICB9XG4gIH1cbn1cblxuLyoqXG4gKiBVcGRhdGUgcGFja2FnZS5qc29uIGFuZCBjb3B5IGFkZG9ucyBpbnRvIHBlciBwbGF0Zm9ybSBwYWNrYWdlc1xuICovXG5leHBvcnQgaW50ZXJmYWNlIFByZVB1Ymxpc2hPcHRpb25zIHtcbiAgLyoqXG4gICAqIFRoZSB3b3JraW5nIGRpcmVjdG9yeSBvZiB3aGVyZSBuYXBpIGNvbW1hbmQgd2lsbCBiZSBleGVjdXRlZCBpbiwgYWxsIG90aGVyIHBhdGhzIG9wdGlvbnMgYXJlIHJlbGF0aXZlIHRvIHRoaXMgcGF0aFxuICAgKlxuICAgKiBAZGVmYXVsdCBwcm9jZXNzLmN3ZCgpXG4gICAqL1xuICBjd2Q/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gYG5hcGlgIGNvbmZpZyBqc29uIGZpbGVcbiAgICovXG4gIGNvbmZpZ1BhdGg/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gYHBhY2thZ2UuanNvbmBcbiAgICpcbiAgICogQGRlZmF1bHQgJ3BhY2thZ2UuanNvbidcbiAgICovXG4gIHBhY2thZ2VKc29uUGF0aD86IHN0cmluZ1xuICAvKipcbiAgICogUGF0aCB0byB0aGUgZm9sZGVyIHdoZXJlIHRoZSBucG0gcGFja2FnZXMgcHV0XG4gICAqXG4gICAqIEBkZWZhdWx0ICducG0nXG4gICAqL1xuICBucG1EaXI/OiBzdHJpbmdcbiAgLyoqXG4gICAqIGdpdCB0YWcgc3R5bGUsIGBucG1gIG9yIGBsZXJuYWBcbiAgICpcbiAgICogQGRlZmF1bHQgJ2xlcm5hJ1xuICAgKi9cbiAgdGFnU3R5bGU/OiAnbnBtJyB8ICdsZXJuYSdcbiAgLyoqXG4gICAqIFdoZXRoZXIgY3JlYXRlIEdpdEh1YiByZWxlYXNlXG4gICAqXG4gICAqIEBkZWZhdWx0IHRydWVcbiAgICovXG4gIGdoUmVsZWFzZT86IGJvb2xlYW5cbiAgLyoqXG4gICAqIEdpdEh1YiByZWxlYXNlIG5hbWVcbiAgICovXG4gIGdoUmVsZWFzZU5hbWU/OiBzdHJpbmdcbiAgLyoqXG4gICAqIEV4aXN0aW5nIEdpdEh1YiByZWxlYXNlIGlkXG4gICAqL1xuICBnaFJlbGVhc2VJZD86IHN0cmluZ1xuICAvKipcbiAgICogV2hldGhlciBza2lwIG9wdGlvbmFsRGVwZW5kZW5jaWVzIHBhY2thZ2VzIHB1Ymxpc2hcbiAgICpcbiAgICogQGRlZmF1bHQgZmFsc2VcbiAgICovXG4gIHNraXBPcHRpb25hbFB1Ymxpc2g/OiBib29sZWFuXG4gIC8qKlxuICAgKiBEcnkgcnVuIHdpdGhvdXQgdG91Y2hpbmcgZmlsZSBzeXN0ZW1cbiAgICpcbiAgICogQGRlZmF1bHQgZmFsc2VcbiAgICovXG4gIGRyeVJ1bj86IGJvb2xlYW5cbn1cblxuZXhwb3J0IGZ1bmN0aW9uIGFwcGx5RGVmYXVsdFByZVB1Ymxpc2hPcHRpb25zKG9wdGlvbnM6IFByZVB1Ymxpc2hPcHRpb25zKSB7XG4gIHJldHVybiB7XG4gICAgY3dkOiBwcm9jZXNzLmN3ZCgpLFxuICAgIHBhY2thZ2VKc29uUGF0aDogJ3BhY2thZ2UuanNvbicsXG4gICAgbnBtRGlyOiAnbnBtJyxcbiAgICB0YWdTdHlsZTogJ2xlcm5hJyxcbiAgICBnaFJlbGVhc2U6IHRydWUsXG4gICAgc2tpcE9wdGlvbmFsUHVibGlzaDogZmFsc2UsXG4gICAgZHJ5UnVuOiBmYWxzZSxcbiAgICAuLi5vcHRpb25zLFxuICB9XG59XG4iLCIvLyBUaGlzIGZpbGUgaXMgZ2VuZXJhdGVkIGJ5IGNvZGVnZW4vaW5kZXgudHNcbi8vIERvIG5vdCBlZGl0IHRoaXMgZmlsZSBtYW51YWxseVxuaW1wb3J0IHsgQ29tbWFuZCwgT3B0aW9uIH0gZnJvbSAnY2xpcGFuaW9uJ1xuXG5leHBvcnQgYWJzdHJhY3QgY2xhc3MgQmFzZVZlcnNpb25Db21tYW5kIGV4dGVuZHMgQ29tbWFuZCB7XG4gIHN0YXRpYyBwYXRocyA9IFtbJ3ZlcnNpb24nXV1cblxuICBzdGF0aWMgdXNhZ2UgPSBDb21tYW5kLlVzYWdlKHtcbiAgICBkZXNjcmlwdGlvbjogJ1VwZGF0ZSB2ZXJzaW9uIGluIGNyZWF0ZWQgbnBtIHBhY2thZ2VzJyxcbiAgfSlcblxuICBjd2QgPSBPcHRpb24uU3RyaW5nKCctLWN3ZCcsIHByb2Nlc3MuY3dkKCksIHtcbiAgICBkZXNjcmlwdGlvbjpcbiAgICAgICdUaGUgd29ya2luZyBkaXJlY3Rvcnkgb2Ygd2hlcmUgbmFwaSBjb21tYW5kIHdpbGwgYmUgZXhlY3V0ZWQgaW4sIGFsbCBvdGhlciBwYXRocyBvcHRpb25zIGFyZSByZWxhdGl2ZSB0byB0aGlzIHBhdGgnLFxuICB9KVxuXG4gIGNvbmZpZ1BhdGg/OiBzdHJpbmcgPSBPcHRpb24uU3RyaW5nKCctLWNvbmZpZy1wYXRoLC1jJywge1xuICAgIGRlc2NyaXB0aW9uOiAnUGF0aCB0byBgbmFwaWAgY29uZmlnIGpzb24gZmlsZScsXG4gIH0pXG5cbiAgcGFja2FnZUpzb25QYXRoID0gT3B0aW9uLlN0cmluZygnLS1wYWNrYWdlLWpzb24tcGF0aCcsICdwYWNrYWdlLmpzb24nLCB7XG4gICAgZGVzY3JpcHRpb246ICdQYXRoIHRvIGBwYWNrYWdlLmpzb25gJyxcbiAgfSlcblxuICBucG1EaXIgPSBPcHRpb24uU3RyaW5nKCctLW5wbS1kaXInLCAnbnBtJywge1xuICAgIGRlc2NyaXB0aW9uOiAnUGF0aCB0byB0aGUgZm9sZGVyIHdoZXJlIHRoZSBucG0gcGFja2FnZXMgcHV0JyxcbiAgfSlcblxuICBnZXRPcHRpb25zKCkge1xuICAgIHJldHVybiB7XG4gICAgICBjd2Q6IHRoaXMuY3dkLFxuICAgICAgY29uZmlnUGF0aDogdGhpcy5jb25maWdQYXRoLFxuICAgICAgcGFja2FnZUpzb25QYXRoOiB0aGlzLnBhY2thZ2VKc29uUGF0aCxcbiAgICAgIG5wbURpcjogdGhpcy5ucG1EaXIsXG4gICAgfVxuICB9XG59XG5cbi8qKlxuICogVXBkYXRlIHZlcnNpb24gaW4gY3JlYXRlZCBucG0gcGFja2FnZXNcbiAqL1xuZXhwb3J0IGludGVyZmFjZSBWZXJzaW9uT3B0aW9ucyB7XG4gIC8qKlxuICAgKiBUaGUgd29ya2luZyBkaXJlY3Rvcnkgb2Ygd2hlcmUgbmFwaSBjb21tYW5kIHdpbGwgYmUgZXhlY3V0ZWQgaW4sIGFsbCBvdGhlciBwYXRocyBvcHRpb25zIGFyZSByZWxhdGl2ZSB0byB0aGlzIHBhdGhcbiAgICpcbiAgICogQGRlZmF1bHQgcHJvY2Vzcy5jd2QoKVxuICAgKi9cbiAgY3dkPzogc3RyaW5nXG4gIC8qKlxuICAgKiBQYXRoIHRvIGBuYXBpYCBjb25maWcganNvbiBmaWxlXG4gICAqL1xuICBjb25maWdQYXRoPzogc3RyaW5nXG4gIC8qKlxuICAgKiBQYXRoIHRvIGBwYWNrYWdlLmpzb25gXG4gICAqXG4gICAqIEBkZWZhdWx0ICdwYWNrYWdlLmpzb24nXG4gICAqL1xuICBwYWNrYWdlSnNvblBhdGg/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gdGhlIGZvbGRlciB3aGVyZSB0aGUgbnBtIHBhY2thZ2VzIHB1dFxuICAgKlxuICAgKiBAZGVmYXVsdCAnbnBtJ1xuICAgKi9cbiAgbnBtRGlyPzogc3RyaW5nXG59XG5cbmV4cG9ydCBmdW5jdGlvbiBhcHBseURlZmF1bHRWZXJzaW9uT3B0aW9ucyhvcHRpb25zOiBWZXJzaW9uT3B0aW9ucykge1xuICByZXR1cm4ge1xuICAgIGN3ZDogcHJvY2Vzcy5jd2QoKSxcbiAgICBwYWNrYWdlSnNvblBhdGg6ICdwYWNrYWdlLmpzb24nLFxuICAgIG5wbURpcjogJ25wbScsXG4gICAgLi4ub3B0aW9ucyxcbiAgfVxufVxuIiwiaW1wb3J0IHsgam9pbiwgcmVzb2x2ZSB9IGZyb20gJ25vZGU6cGF0aCdcblxuaW1wb3J0IHtcbiAgYXBwbHlEZWZhdWx0VmVyc2lvbk9wdGlvbnMsXG4gIHR5cGUgVmVyc2lvbk9wdGlvbnMsXG59IGZyb20gJy4uL2RlZi92ZXJzaW9uLmpzJ1xuaW1wb3J0IHtcbiAgcmVhZE5hcGlDb25maWcsXG4gIGRlYnVnRmFjdG9yeSxcbiAgdXBkYXRlUGFja2FnZUpzb24sXG59IGZyb20gJy4uL3V0aWxzL2luZGV4LmpzJ1xuXG5jb25zdCBkZWJ1ZyA9IGRlYnVnRmFjdG9yeSgndmVyc2lvbicpXG5cbmV4cG9ydCBhc3luYyBmdW5jdGlvbiB2ZXJzaW9uKHVzZXJPcHRpb25zOiBWZXJzaW9uT3B0aW9ucykge1xuICBjb25zdCBvcHRpb25zID0gYXBwbHlEZWZhdWx0VmVyc2lvbk9wdGlvbnModXNlck9wdGlvbnMpXG4gIGNvbnN0IHBhY2thZ2VKc29uUGF0aCA9IHJlc29sdmUob3B0aW9ucy5jd2QsIG9wdGlvbnMucGFja2FnZUpzb25QYXRoKVxuXG4gIGNvbnN0IGNvbmZpZyA9IGF3YWl0IHJlYWROYXBpQ29uZmlnKFxuICAgIHBhY2thZ2VKc29uUGF0aCxcbiAgICBvcHRpb25zLmNvbmZpZ1BhdGggPyByZXNvbHZlKG9wdGlvbnMuY3dkLCBvcHRpb25zLmNvbmZpZ1BhdGgpIDogdW5kZWZpbmVkLFxuICApXG5cbiAgZm9yIChjb25zdCB0YXJnZXQgb2YgY29uZmlnLnRhcmdldHMpIHtcbiAgICBjb25zdCBwa2dEaXIgPSByZXNvbHZlKG9wdGlvbnMuY3dkLCBvcHRpb25zLm5wbURpciwgdGFyZ2V0LnBsYXRmb3JtQXJjaEFCSSlcblxuICAgIGRlYnVnKGBVcGRhdGUgdmVyc2lvbiB0byAlaSBpbiBbJWldYCwgY29uZmlnLnBhY2thZ2VKc29uLnZlcnNpb24sIHBrZ0RpcilcbiAgICBhd2FpdCB1cGRhdGVQYWNrYWdlSnNvbihqb2luKHBrZ0RpciwgJ3BhY2thZ2UuanNvbicpLCB7XG4gICAgICB2ZXJzaW9uOiBjb25maWcucGFja2FnZUpzb24udmVyc2lvbixcbiAgICB9KVxuICB9XG59XG4iLCJpbXBvcnQgeyBleGVjU3luYyB9IGZyb20gJ25vZGU6Y2hpbGRfcHJvY2VzcydcbmltcG9ydCB7IGV4aXN0c1N5bmMsIHN0YXRTeW5jIH0gZnJvbSAnbm9kZTpmcydcbmltcG9ydCB7IGpvaW4sIHJlc29sdmUgfSBmcm9tICdub2RlOnBhdGgnXG5cbmltcG9ydCB7IE9jdG9raXQgfSBmcm9tICdAb2N0b2tpdC9yZXN0J1xuXG5pbXBvcnQge1xuICBhcHBseURlZmF1bHRQcmVQdWJsaXNoT3B0aW9ucyxcbiAgdHlwZSBQcmVQdWJsaXNoT3B0aW9ucyxcbn0gZnJvbSAnLi4vZGVmL3ByZS1wdWJsaXNoLmpzJ1xuaW1wb3J0IHtcbiAgcmVhZEZpbGVBc3luYyxcbiAgcmVhZE5hcGlDb25maWcsXG4gIGRlYnVnRmFjdG9yeSxcbiAgdXBkYXRlUGFja2FnZUpzb24sXG59IGZyb20gJy4uL3V0aWxzL2luZGV4LmpzJ1xuXG5pbXBvcnQgeyB2ZXJzaW9uIH0gZnJvbSAnLi92ZXJzaW9uLmpzJ1xuXG5jb25zdCBkZWJ1ZyA9IGRlYnVnRmFjdG9yeSgncHJlLXB1Ymxpc2gnKVxuXG5pbnRlcmZhY2UgUGFja2FnZUluZm8ge1xuICBuYW1lOiBzdHJpbmdcbiAgdmVyc2lvbjogc3RyaW5nXG4gIHRhZzogc3RyaW5nXG59XG5cbmV4cG9ydCBhc3luYyBmdW5jdGlvbiBwcmVQdWJsaXNoKHVzZXJPcHRpb25zOiBQcmVQdWJsaXNoT3B0aW9ucykge1xuICBkZWJ1ZygnUmVjZWl2ZSBwcmUtcHVibGlzaCBvcHRpb25zOicpXG4gIGRlYnVnKCcgICVPJywgdXNlck9wdGlvbnMpXG5cbiAgY29uc3Qgb3B0aW9ucyA9IGFwcGx5RGVmYXVsdFByZVB1Ymxpc2hPcHRpb25zKHVzZXJPcHRpb25zKVxuXG4gIGNvbnN0IHBhY2thZ2VKc29uUGF0aCA9IHJlc29sdmUob3B0aW9ucy5jd2QsIG9wdGlvbnMucGFja2FnZUpzb25QYXRoKVxuXG4gIGNvbnN0IHsgcGFja2FnZUpzb24sIHRhcmdldHMsIHBhY2thZ2VOYW1lLCBiaW5hcnlOYW1lLCBucG1DbGllbnQgfSA9XG4gICAgYXdhaXQgcmVhZE5hcGlDb25maWcoXG4gICAgICBwYWNrYWdlSnNvblBhdGgsXG4gICAgICBvcHRpb25zLmNvbmZpZ1BhdGggPyByZXNvbHZlKG9wdGlvbnMuY3dkLCBvcHRpb25zLmNvbmZpZ1BhdGgpIDogdW5kZWZpbmVkLFxuICAgIClcblxuICBhc3luYyBmdW5jdGlvbiBjcmVhdGVHaFJlbGVhc2UocGFja2FnZU5hbWU6IHN0cmluZywgdmVyc2lvbjogc3RyaW5nKSB7XG4gICAgaWYgKCFvcHRpb25zLmdoUmVsZWFzZSkge1xuICAgICAgcmV0dXJuIHtcbiAgICAgICAgb3duZXI6IG51bGwsXG4gICAgICAgIHJlcG86IG51bGwsXG4gICAgICAgIHBrZ0luZm86IHsgbmFtZTogbnVsbCwgdmVyc2lvbjogbnVsbCwgdGFnOiBudWxsIH0sXG4gICAgICB9XG4gICAgfVxuICAgIGNvbnN0IHsgcmVwbywgb3duZXIsIHBrZ0luZm8sIG9jdG9raXQgfSA9IGdldFJlcG9JbmZvKHBhY2thZ2VOYW1lLCB2ZXJzaW9uKVxuXG4gICAgaWYgKCFyZXBvIHx8ICFvd25lcikge1xuICAgICAgcmV0dXJuIHtcbiAgICAgICAgb3duZXI6IG51bGwsXG4gICAgICAgIHJlcG86IG51bGwsXG4gICAgICAgIHBrZ0luZm86IHsgbmFtZTogbnVsbCwgdmVyc2lvbjogbnVsbCwgdGFnOiBudWxsIH0sXG4gICAgICB9XG4gICAgfVxuXG4gICAgaWYgKCFvcHRpb25zLmRyeVJ1bikge1xuICAgICAgdHJ5IHtcbiAgICAgICAgYXdhaXQgb2N0b2tpdC5yZXBvcy5jcmVhdGVSZWxlYXNlKHtcbiAgICAgICAgICBvd25lcixcbiAgICAgICAgICByZXBvLFxuICAgICAgICAgIHRhZ19uYW1lOiBwa2dJbmZvLnRhZyxcbiAgICAgICAgICBuYW1lOiBvcHRpb25zLmdoUmVsZWFzZU5hbWUsXG4gICAgICAgICAgcHJlcmVsZWFzZTpcbiAgICAgICAgICAgIHZlcnNpb24uaW5jbHVkZXMoJ2FscGhhJykgfHxcbiAgICAgICAgICAgIHZlcnNpb24uaW5jbHVkZXMoJ2JldGEnKSB8fFxuICAgICAgICAgICAgdmVyc2lvbi5pbmNsdWRlcygncmMnKSxcbiAgICAgICAgfSlcbiAgICAgIH0gY2F0Y2ggKGUpIHtcbiAgICAgICAgZGVidWcoXG4gICAgICAgICAgYFBhcmFtczogJHtKU09OLnN0cmluZ2lmeShcbiAgICAgICAgICAgIHsgb3duZXIsIHJlcG8sIHRhZ19uYW1lOiBwa2dJbmZvLnRhZyB9LFxuICAgICAgICAgICAgbnVsbCxcbiAgICAgICAgICAgIDIsXG4gICAgICAgICAgKX1gLFxuICAgICAgICApXG4gICAgICAgIGNvbnNvbGUuZXJyb3IoZSlcbiAgICAgIH1cbiAgICB9XG4gICAgcmV0dXJuIHsgb3duZXIsIHJlcG8sIHBrZ0luZm8sIG9jdG9raXQgfVxuICB9XG5cbiAgZnVuY3Rpb24gZ2V0UmVwb0luZm8ocGFja2FnZU5hbWU6IHN0cmluZywgdmVyc2lvbjogc3RyaW5nKSB7XG4gICAgY29uc3QgaGVhZENvbW1pdCA9IGV4ZWNTeW5jKCdnaXQgbG9nIC0xIC0tcHJldHR5PSVCJywge1xuICAgICAgZW5jb2Rpbmc6ICd1dGYtOCcsXG4gICAgfSkudHJpbSgpXG5cbiAgICBjb25zdCB7IEdJVEhVQl9SRVBPU0lUT1JZIH0gPSBwcm9jZXNzLmVudlxuICAgIGlmICghR0lUSFVCX1JFUE9TSVRPUlkpIHtcbiAgICAgIHJldHVybiB7XG4gICAgICAgIG93bmVyOiBudWxsLFxuICAgICAgICByZXBvOiBudWxsLFxuICAgICAgICBwa2dJbmZvOiB7IG5hbWU6IG51bGwsIHZlcnNpb246IG51bGwsIHRhZzogbnVsbCB9LFxuICAgICAgfVxuICAgIH1cbiAgICBkZWJ1ZyhgR2l0aHViIHJlcG9zaXRvcnk6ICR7R0lUSFVCX1JFUE9TSVRPUll9YClcbiAgICBjb25zdCBbb3duZXIsIHJlcG9dID0gR0lUSFVCX1JFUE9TSVRPUlkuc3BsaXQoJy8nKVxuICAgIGNvbnN0IG9jdG9raXQgPSBuZXcgT2N0b2tpdCh7XG4gICAgICBhdXRoOiBwcm9jZXNzLmVudi5HSVRIVUJfVE9LRU4sXG4gICAgfSlcbiAgICBsZXQgcGtnSW5mbzogUGFja2FnZUluZm8gfCB1bmRlZmluZWRcbiAgICBpZiAob3B0aW9ucy50YWdTdHlsZSA9PT0gJ2xlcm5hJykge1xuICAgICAgY29uc3QgcGFja2FnZXNUb1B1Ymxpc2ggPSBoZWFkQ29tbWl0XG4gICAgICAgIC5zcGxpdCgnXFxuJylcbiAgICAgICAgLm1hcCgobGluZSkgPT4gbGluZS50cmltKCkpXG4gICAgICAgIC5maWx0ZXIoKGxpbmUsIGluZGV4KSA9PiBsaW5lLmxlbmd0aCAmJiBpbmRleClcbiAgICAgICAgLm1hcCgobGluZSkgPT4gbGluZS5zdWJzdHJpbmcoMikpXG4gICAgICAgIC5tYXAocGFyc2VUYWcpXG5cbiAgICAgIHBrZ0luZm8gPSBwYWNrYWdlc1RvUHVibGlzaC5maW5kKFxuICAgICAgICAocGtnSW5mbykgPT4gcGtnSW5mby5uYW1lID09PSBwYWNrYWdlTmFtZSxcbiAgICAgIClcblxuICAgICAgaWYgKCFwa2dJbmZvKSB7XG4gICAgICAgIHRocm93IG5ldyBUeXBlRXJyb3IoXG4gICAgICAgICAgYE5vIHJlbGVhc2UgY29tbWl0IGZvdW5kIHdpdGggJHtwYWNrYWdlTmFtZX0sIG9yaWdpbmFsIGNvbW1pdCBpbmZvOiAke2hlYWRDb21taXR9YCxcbiAgICAgICAgKVxuICAgICAgfVxuICAgIH0gZWxzZSB7XG4gICAgICBwa2dJbmZvID0ge1xuICAgICAgICB0YWc6IGB2JHt2ZXJzaW9ufWAsXG4gICAgICAgIHZlcnNpb24sXG4gICAgICAgIG5hbWU6IHBhY2thZ2VOYW1lLFxuICAgICAgfVxuICAgIH1cbiAgICByZXR1cm4geyBvd25lciwgcmVwbywgcGtnSW5mbywgb2N0b2tpdCB9XG4gIH1cblxuICBpZiAoIW9wdGlvbnMuZHJ5UnVuKSB7XG4gICAgYXdhaXQgdmVyc2lvbih1c2VyT3B0aW9ucylcbiAgICBhd2FpdCB1cGRhdGVQYWNrYWdlSnNvbihwYWNrYWdlSnNvblBhdGgsIHtcbiAgICAgIG9wdGlvbmFsRGVwZW5kZW5jaWVzOiB0YXJnZXRzLnJlZHVjZShcbiAgICAgICAgKGRlcHMsIHRhcmdldCkgPT4ge1xuICAgICAgICAgIGRlcHNbYCR7cGFja2FnZU5hbWV9LSR7dGFyZ2V0LnBsYXRmb3JtQXJjaEFCSX1gXSA9IHBhY2thZ2VKc29uLnZlcnNpb25cblxuICAgICAgICAgIHJldHVybiBkZXBzXG4gICAgICAgIH0sXG4gICAgICAgIHt9IGFzIFJlY29yZDxzdHJpbmcsIHN0cmluZz4sXG4gICAgICApLFxuICAgIH0pXG4gIH1cblxuICBjb25zdCB7IG93bmVyLCByZXBvLCBwa2dJbmZvLCBvY3Rva2l0IH0gPSBvcHRpb25zLmdoUmVsZWFzZUlkXG4gICAgPyBnZXRSZXBvSW5mbyhwYWNrYWdlTmFtZSwgcGFja2FnZUpzb24udmVyc2lvbilcbiAgICA6IGF3YWl0IGNyZWF0ZUdoUmVsZWFzZShwYWNrYWdlTmFtZSwgcGFja2FnZUpzb24udmVyc2lvbilcblxuICBmb3IgKGNvbnN0IHRhcmdldCBvZiB0YXJnZXRzKSB7XG4gICAgY29uc3QgcGtnRGlyID0gcmVzb2x2ZShcbiAgICAgIG9wdGlvbnMuY3dkLFxuICAgICAgb3B0aW9ucy5ucG1EaXIsXG4gICAgICBgJHt0YXJnZXQucGxhdGZvcm1BcmNoQUJJfWAsXG4gICAgKVxuICAgIGNvbnN0IGV4dCA9XG4gICAgICB0YXJnZXQucGxhdGZvcm0gPT09ICd3YXNpJyB8fCB0YXJnZXQucGxhdGZvcm0gPT09ICd3YXNtJyA/ICd3YXNtJyA6ICdub2RlJ1xuICAgIGNvbnN0IGZpbGVuYW1lID0gYCR7YmluYXJ5TmFtZX0uJHt0YXJnZXQucGxhdGZvcm1BcmNoQUJJfS4ke2V4dH1gXG4gICAgY29uc3QgZHN0UGF0aCA9IGpvaW4ocGtnRGlyLCBmaWxlbmFtZSlcblxuICAgIGlmICghb3B0aW9ucy5kcnlSdW4pIHtcbiAgICAgIGlmICghZXhpc3RzU3luYyhkc3RQYXRoKSkge1xuICAgICAgICBkZWJ1Zy53YXJuKGAlcyBkb2Vzbid0IGV4aXN0YCwgZHN0UGF0aClcbiAgICAgICAgY29udGludWVcbiAgICAgIH1cblxuICAgICAgaWYgKCFvcHRpb25zLnNraXBPcHRpb25hbFB1Ymxpc2gpIHtcbiAgICAgICAgdHJ5IHtcbiAgICAgICAgICBjb25zdCBvdXRwdXQgPSBleGVjU3luYyhgJHtucG1DbGllbnR9IHB1Ymxpc2hgLCB7XG4gICAgICAgICAgICBjd2Q6IHBrZ0RpcixcbiAgICAgICAgICAgIGVudjogcHJvY2Vzcy5lbnYsXG4gICAgICAgICAgICBzdGRpbzogJ3BpcGUnLFxuICAgICAgICAgIH0pXG4gICAgICAgICAgcHJvY2Vzcy5zdGRvdXQud3JpdGUob3V0cHV0KVxuICAgICAgICB9IGNhdGNoIChlKSB7XG4gICAgICAgICAgaWYgKFxuICAgICAgICAgICAgZSBpbnN0YW5jZW9mIEVycm9yICYmXG4gICAgICAgICAgICBlLm1lc3NhZ2UuaW5jbHVkZXMoXG4gICAgICAgICAgICAgICdZb3UgY2Fubm90IHB1Ymxpc2ggb3ZlciB0aGUgcHJldmlvdXNseSBwdWJsaXNoZWQgdmVyc2lvbnMnLFxuICAgICAgICAgICAgKVxuICAgICAgICAgICkge1xuICAgICAgICAgICAgY29uc29sZS5pbmZvKGUubWVzc2FnZSlcbiAgICAgICAgICAgIGRlYnVnLndhcm4oYCR7cGtnRGlyfSBoYXMgYmVlbiBwdWJsaXNoZWQsIHNraXBwaW5nYClcbiAgICAgICAgICB9IGVsc2Uge1xuICAgICAgICAgICAgdGhyb3cgZVxuICAgICAgICAgIH1cbiAgICAgICAgfVxuICAgICAgfVxuXG4gICAgICBpZiAob3B0aW9ucy5naFJlbGVhc2UgJiYgcmVwbyAmJiBvd25lcikge1xuICAgICAgICBkZWJ1Zy5pbmZvKGBDcmVhdGluZyBHaXRIdWIgcmVsZWFzZSAke3BrZ0luZm8udGFnfWApXG4gICAgICAgIHRyeSB7XG4gICAgICAgICAgY29uc3QgcmVsZWFzZUlkID0gb3B0aW9ucy5naFJlbGVhc2VJZFxuICAgICAgICAgICAgPyBOdW1iZXIob3B0aW9ucy5naFJlbGVhc2VJZClcbiAgICAgICAgICAgIDogKFxuICAgICAgICAgICAgICAgIGF3YWl0IG9jdG9raXQhLnJlcG9zLmdldFJlbGVhc2VCeVRhZyh7XG4gICAgICAgICAgICAgICAgICByZXBvOiByZXBvLFxuICAgICAgICAgICAgICAgICAgb3duZXI6IG93bmVyLFxuICAgICAgICAgICAgICAgICAgdGFnOiBwa2dJbmZvLnRhZyxcbiAgICAgICAgICAgICAgICB9KVxuICAgICAgICAgICAgICApLmRhdGEuaWRcbiAgICAgICAgICBjb25zdCBkc3RGaWxlU3RhdHMgPSBzdGF0U3luYyhkc3RQYXRoKVxuICAgICAgICAgIGNvbnN0IGFzc2V0SW5mbyA9IGF3YWl0IG9jdG9raXQhLnJlcG9zLnVwbG9hZFJlbGVhc2VBc3NldCh7XG4gICAgICAgICAgICBvd25lcjogb3duZXIsXG4gICAgICAgICAgICByZXBvOiByZXBvLFxuICAgICAgICAgICAgbmFtZTogZmlsZW5hbWUsXG4gICAgICAgICAgICByZWxlYXNlX2lkOiByZWxlYXNlSWQsXG4gICAgICAgICAgICBtZWRpYVR5cGU6IHsgZm9ybWF0OiAncmF3JyB9LFxuICAgICAgICAgICAgaGVhZGVyczoge1xuICAgICAgICAgICAgICAnY29udGVudC1sZW5ndGgnOiBkc3RGaWxlU3RhdHMuc2l6ZSxcbiAgICAgICAgICAgICAgJ2NvbnRlbnQtdHlwZSc6ICdhcHBsaWNhdGlvbi9vY3RldC1zdHJlYW0nLFxuICAgICAgICAgICAgfSxcbiAgICAgICAgICAgIC8vIEB0cy1leHBlY3QtZXJyb3Igb2N0b2tpdCB0eXBlcyBhcmUgd3JvbmdcbiAgICAgICAgICAgIGRhdGE6IGF3YWl0IHJlYWRGaWxlQXN5bmMoZHN0UGF0aCksXG4gICAgICAgICAgfSlcbiAgICAgICAgICBkZWJ1Zy5pbmZvKGBHaXRIdWIgcmVsZWFzZSBjcmVhdGVkYClcbiAgICAgICAgICBkZWJ1Zy5pbmZvKGBEb3dubG9hZCBVUkw6ICVzYCwgYXNzZXRJbmZvLmRhdGEuYnJvd3Nlcl9kb3dubG9hZF91cmwpXG4gICAgICAgIH0gY2F0Y2ggKGUpIHtcbiAgICAgICAgICBkZWJ1Zy5lcnJvcihcbiAgICAgICAgICAgIGBQYXJhbTogJHtKU09OLnN0cmluZ2lmeShcbiAgICAgICAgICAgICAgeyBvd25lciwgcmVwbywgdGFnOiBwa2dJbmZvLnRhZywgZmlsZW5hbWU6IGRzdFBhdGggfSxcbiAgICAgICAgICAgICAgbnVsbCxcbiAgICAgICAgICAgICAgMixcbiAgICAgICAgICAgICl9YCxcbiAgICAgICAgICApXG4gICAgICAgICAgZGVidWcuZXJyb3IoZSlcbiAgICAgICAgfVxuICAgICAgfVxuICAgIH1cbiAgfVxufVxuXG5mdW5jdGlvbiBwYXJzZVRhZyh0YWc6IHN0cmluZykge1xuICBjb25zdCBzZWdtZW50cyA9IHRhZy5zcGxpdCgnQCcpXG4gIGNvbnN0IHZlcnNpb24gPSBzZWdtZW50cy5wb3AoKSFcbiAgY29uc3QgbmFtZSA9IHNlZ21lbnRzLmpvaW4oJ0AnKVxuXG4gIHJldHVybiB7XG4gICAgbmFtZSxcbiAgICB2ZXJzaW9uLFxuICAgIHRhZyxcbiAgfVxufVxuIiwiLy8gVGhpcyBmaWxlIGlzIGdlbmVyYXRlZCBieSBjb2RlZ2VuL2luZGV4LnRzXG4vLyBEbyBub3QgZWRpdCB0aGlzIGZpbGUgbWFudWFsbHlcbmltcG9ydCB7IENvbW1hbmQsIE9wdGlvbiB9IGZyb20gJ2NsaXBhbmlvbidcblxuZXhwb3J0IGFic3RyYWN0IGNsYXNzIEJhc2VVbml2ZXJzYWxpemVDb21tYW5kIGV4dGVuZHMgQ29tbWFuZCB7XG4gIHN0YXRpYyBwYXRocyA9IFtbJ3VuaXZlcnNhbGl6ZSddXVxuXG4gIHN0YXRpYyB1c2FnZSA9IENvbW1hbmQuVXNhZ2Uoe1xuICAgIGRlc2NyaXB0aW9uOiAnQ29tYmlsZSBidWlsdCBiaW5hcmllcyBpbnRvIG9uZSB1bml2ZXJzYWwgYmluYXJ5JyxcbiAgfSlcblxuICBjd2QgPSBPcHRpb24uU3RyaW5nKCctLWN3ZCcsIHByb2Nlc3MuY3dkKCksIHtcbiAgICBkZXNjcmlwdGlvbjpcbiAgICAgICdUaGUgd29ya2luZyBkaXJlY3Rvcnkgb2Ygd2hlcmUgbmFwaSBjb21tYW5kIHdpbGwgYmUgZXhlY3V0ZWQgaW4sIGFsbCBvdGhlciBwYXRocyBvcHRpb25zIGFyZSByZWxhdGl2ZSB0byB0aGlzIHBhdGgnLFxuICB9KVxuXG4gIGNvbmZpZ1BhdGg/OiBzdHJpbmcgPSBPcHRpb24uU3RyaW5nKCctLWNvbmZpZy1wYXRoLC1jJywge1xuICAgIGRlc2NyaXB0aW9uOiAnUGF0aCB0byBgbmFwaWAgY29uZmlnIGpzb24gZmlsZScsXG4gIH0pXG5cbiAgcGFja2FnZUpzb25QYXRoID0gT3B0aW9uLlN0cmluZygnLS1wYWNrYWdlLWpzb24tcGF0aCcsICdwYWNrYWdlLmpzb24nLCB7XG4gICAgZGVzY3JpcHRpb246ICdQYXRoIHRvIGBwYWNrYWdlLmpzb25gJyxcbiAgfSlcblxuICBvdXRwdXREaXIgPSBPcHRpb24uU3RyaW5nKCctLW91dHB1dC1kaXIsLW8nLCAnLi8nLCB7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnUGF0aCB0byB0aGUgZm9sZGVyIHdoZXJlIGFsbCBidWlsdCBgLm5vZGVgIGZpbGVzIHB1dCwgc2FtZSBhcyBgLS1vdXRwdXQtZGlyYCBvZiBidWlsZCBjb21tYW5kJyxcbiAgfSlcblxuICBnZXRPcHRpb25zKCkge1xuICAgIHJldHVybiB7XG4gICAgICBjd2Q6IHRoaXMuY3dkLFxuICAgICAgY29uZmlnUGF0aDogdGhpcy5jb25maWdQYXRoLFxuICAgICAgcGFja2FnZUpzb25QYXRoOiB0aGlzLnBhY2thZ2VKc29uUGF0aCxcbiAgICAgIG91dHB1dERpcjogdGhpcy5vdXRwdXREaXIsXG4gICAgfVxuICB9XG59XG5cbi8qKlxuICogQ29tYmlsZSBidWlsdCBiaW5hcmllcyBpbnRvIG9uZSB1bml2ZXJzYWwgYmluYXJ5XG4gKi9cbmV4cG9ydCBpbnRlcmZhY2UgVW5pdmVyc2FsaXplT3B0aW9ucyB7XG4gIC8qKlxuICAgKiBUaGUgd29ya2luZyBkaXJlY3Rvcnkgb2Ygd2hlcmUgbmFwaSBjb21tYW5kIHdpbGwgYmUgZXhlY3V0ZWQgaW4sIGFsbCBvdGhlciBwYXRocyBvcHRpb25zIGFyZSByZWxhdGl2ZSB0byB0aGlzIHBhdGhcbiAgICpcbiAgICogQGRlZmF1bHQgcHJvY2Vzcy5jd2QoKVxuICAgKi9cbiAgY3dkPzogc3RyaW5nXG4gIC8qKlxuICAgKiBQYXRoIHRvIGBuYXBpYCBjb25maWcganNvbiBmaWxlXG4gICAqL1xuICBjb25maWdQYXRoPzogc3RyaW5nXG4gIC8qKlxuICAgKiBQYXRoIHRvIGBwYWNrYWdlLmpzb25gXG4gICAqXG4gICAqIEBkZWZhdWx0ICdwYWNrYWdlLmpzb24nXG4gICAqL1xuICBwYWNrYWdlSnNvblBhdGg/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gdGhlIGZvbGRlciB3aGVyZSBhbGwgYnVpbHQgYC5ub2RlYCBmaWxlcyBwdXQsIHNhbWUgYXMgYC0tb3V0cHV0LWRpcmAgb2YgYnVpbGQgY29tbWFuZFxuICAgKlxuICAgKiBAZGVmYXVsdCAnLi8nXG4gICAqL1xuICBvdXRwdXREaXI/OiBzdHJpbmdcbn1cblxuZXhwb3J0IGZ1bmN0aW9uIGFwcGx5RGVmYXVsdFVuaXZlcnNhbGl6ZU9wdGlvbnMob3B0aW9uczogVW5pdmVyc2FsaXplT3B0aW9ucykge1xuICByZXR1cm4ge1xuICAgIGN3ZDogcHJvY2Vzcy5jd2QoKSxcbiAgICBwYWNrYWdlSnNvblBhdGg6ICdwYWNrYWdlLmpzb24nLFxuICAgIG91dHB1dERpcjogJy4vJyxcbiAgICAuLi5vcHRpb25zLFxuICB9XG59XG4iLCJpbXBvcnQgeyBzcGF3blN5bmMgfSBmcm9tICdub2RlOmNoaWxkX3Byb2Nlc3MnXG5pbXBvcnQgeyBqb2luLCByZXNvbHZlIH0gZnJvbSAnbm9kZTpwYXRoJ1xuXG5pbXBvcnQge1xuICBhcHBseURlZmF1bHRVbml2ZXJzYWxpemVPcHRpb25zLFxuICB0eXBlIFVuaXZlcnNhbGl6ZU9wdGlvbnMsXG59IGZyb20gJy4uL2RlZi91bml2ZXJzYWxpemUuanMnXG5pbXBvcnQgeyByZWFkTmFwaUNvbmZpZyB9IGZyb20gJy4uL3V0aWxzL2NvbmZpZy5qcydcbmltcG9ydCB7IGRlYnVnRmFjdG9yeSB9IGZyb20gJy4uL3V0aWxzL2xvZy5qcydcbmltcG9ydCB7IGZpbGVFeGlzdHMgfSBmcm9tICcuLi91dGlscy9taXNjLmpzJ1xuaW1wb3J0IHsgVW5pQXJjaHNCeVBsYXRmb3JtIH0gZnJvbSAnLi4vdXRpbHMvdGFyZ2V0LmpzJ1xuXG5jb25zdCBkZWJ1ZyA9IGRlYnVnRmFjdG9yeSgndW5pdmVyc2FsaXplJylcblxuY29uc3QgdW5pdmVyc2FsaXplcnM6IFBhcnRpYWw8XG4gIFJlY29yZDxOb2RlSlMuUGxhdGZvcm0sIChpbnB1dHM6IHN0cmluZ1tdLCBvdXRwdXQ6IHN0cmluZykgPT4gdm9pZD5cbj4gPSB7XG4gIGRhcndpbjogKGlucHV0cywgb3V0cHV0KSA9PiB7XG4gICAgc3Bhd25TeW5jKCdsaXBvJywgWyctY3JlYXRlJywgJy1vdXRwdXQnLCBvdXRwdXQsIC4uLmlucHV0c10sIHtcbiAgICAgIHN0ZGlvOiAnaW5oZXJpdCcsXG4gICAgfSlcbiAgfSxcbn1cblxuZXhwb3J0IGFzeW5jIGZ1bmN0aW9uIHVuaXZlcnNhbGl6ZUJpbmFyaWVzKHVzZXJPcHRpb25zOiBVbml2ZXJzYWxpemVPcHRpb25zKSB7XG4gIGNvbnN0IG9wdGlvbnMgPSBhcHBseURlZmF1bHRVbml2ZXJzYWxpemVPcHRpb25zKHVzZXJPcHRpb25zKVxuXG4gIGNvbnN0IHBhY2thZ2VKc29uUGF0aCA9IGpvaW4ob3B0aW9ucy5jd2QsIG9wdGlvbnMucGFja2FnZUpzb25QYXRoKVxuXG4gIGNvbnN0IGNvbmZpZyA9IGF3YWl0IHJlYWROYXBpQ29uZmlnKFxuICAgIHBhY2thZ2VKc29uUGF0aCxcbiAgICBvcHRpb25zLmNvbmZpZ1BhdGggPyByZXNvbHZlKG9wdGlvbnMuY3dkLCBvcHRpb25zLmNvbmZpZ1BhdGgpIDogdW5kZWZpbmVkLFxuICApXG5cbiAgY29uc3QgdGFyZ2V0ID0gY29uZmlnLnRhcmdldHMuZmluZChcbiAgICAodCkgPT4gdC5wbGF0Zm9ybSA9PT0gcHJvY2Vzcy5wbGF0Zm9ybSAmJiB0LmFyY2ggPT09ICd1bml2ZXJzYWwnLFxuICApXG5cbiAgaWYgKCF0YXJnZXQpIHtcbiAgICB0aHJvdyBuZXcgRXJyb3IoXG4gICAgICBgJ3VuaXZlcnNhbCcgYXJjaCBmb3IgcGxhdGZvcm0gJyR7cHJvY2Vzcy5wbGF0Zm9ybX0nIG5vdCBmb3VuZCBpbiBjb25maWchYCxcbiAgICApXG4gIH1cblxuICBjb25zdCBzcmNGaWxlcyA9IFVuaUFyY2hzQnlQbGF0Zm9ybVtwcm9jZXNzLnBsYXRmb3JtXT8ubWFwKChhcmNoKSA9PlxuICAgIHJlc29sdmUoXG4gICAgICBvcHRpb25zLmN3ZCxcbiAgICAgIG9wdGlvbnMub3V0cHV0RGlyLFxuICAgICAgYCR7Y29uZmlnLmJpbmFyeU5hbWV9LiR7cHJvY2Vzcy5wbGF0Zm9ybX0tJHthcmNofS5ub2RlYCxcbiAgICApLFxuICApXG5cbiAgaWYgKCFzcmNGaWxlcyB8fCAhdW5pdmVyc2FsaXplcnNbcHJvY2Vzcy5wbGF0Zm9ybV0pIHtcbiAgICB0aHJvdyBuZXcgRXJyb3IoXG4gICAgICBgJ3VuaXZlcnNhbCcgYXJjaCBmb3IgcGxhdGZvcm0gJyR7cHJvY2Vzcy5wbGF0Zm9ybX0nIG5vdCBzdXBwb3J0ZWQuYCxcbiAgICApXG4gIH1cblxuICBkZWJ1ZyhgTG9va2luZyB1cCBzb3VyY2UgYmluYXJpZXMgdG8gY29tYmluZTogYClcbiAgZGVidWcoJyAgJU8nLCBzcmNGaWxlcylcblxuICBjb25zdCBzcmNGaWxlTG9va3VwID0gYXdhaXQgUHJvbWlzZS5hbGwoc3JjRmlsZXMubWFwKChmKSA9PiBmaWxlRXhpc3RzKGYpKSlcblxuICBjb25zdCBub3RGb3VuZEZpbGVzID0gc3JjRmlsZXMuZmlsdGVyKChfLCBpKSA9PiAhc3JjRmlsZUxvb2t1cFtpXSlcblxuICBpZiAobm90Rm91bmRGaWxlcy5sZW5ndGgpIHtcbiAgICB0aHJvdyBuZXcgRXJyb3IoXG4gICAgICBgU29tZSBiaW5hcnkgZmlsZXMgd2VyZSBub3QgZm91bmQ6ICR7SlNPTi5zdHJpbmdpZnkobm90Rm91bmRGaWxlcyl9YCxcbiAgICApXG4gIH1cblxuICBjb25zdCBvdXRwdXQgPSByZXNvbHZlKFxuICAgIG9wdGlvbnMuY3dkLFxuICAgIG9wdGlvbnMub3V0cHV0RGlyLFxuICAgIGAke2NvbmZpZy5iaW5hcnlOYW1lfS4ke3Byb2Nlc3MucGxhdGZvcm19LXVuaXZlcnNhbC5ub2RlYCxcbiAgKVxuXG4gIHVuaXZlcnNhbGl6ZXJzW3Byb2Nlc3MucGxhdGZvcm1dPy4oc3JjRmlsZXMsIG91dHB1dClcblxuICBkZWJ1ZyhgUHJvZHVjZWQgdW5pdmVyc2FsIGJpbmFyeTogJHtvdXRwdXR9YClcbn1cbiIsImltcG9ydCB7IENvbW1hbmQgfSBmcm9tICdjbGlwYW5pb24nXG5cbmltcG9ydCB7IGNvbGxlY3RBcnRpZmFjdHMgfSBmcm9tICcuLi9hcGkvYXJ0aWZhY3RzLmpzJ1xuaW1wb3J0IHsgQmFzZUFydGlmYWN0c0NvbW1hbmQgfSBmcm9tICcuLi9kZWYvYXJ0aWZhY3RzLmpzJ1xuXG5leHBvcnQgY2xhc3MgQXJ0aWZhY3RzQ29tbWFuZCBleHRlbmRzIEJhc2VBcnRpZmFjdHNDb21tYW5kIHtcbiAgc3RhdGljIHVzYWdlID0gQ29tbWFuZC5Vc2FnZSh7XG4gICAgZGVzY3JpcHRpb246ICdDb3B5IGFydGlmYWN0cyBmcm9tIEdpdGh1YiBBY3Rpb25zIGludG8gc3BlY2lmaWVkIGRpcicsXG4gICAgZXhhbXBsZXM6IFtcbiAgICAgIFtcbiAgICAgICAgJyQwIGFydGlmYWN0cyAtLW91dHB1dC1kaXIgLi9hcnRpZmFjdHMgLS1kaXN0IC4vbnBtJyxcbiAgICAgICAgYENvcHkgW2JpbmFyeU5hbWVdLltwbGF0Zm9ybV0ubm9kZSB1bmRlciBjdXJyZW50IGRpciguKSBpbnRvIHBhY2thZ2VzIHVuZGVyIG5wbSBkaXIuXG5lLmc6IGluZGV4LmxpbnV4LXg2NC1nbnUubm9kZSAtLT4gLi9ucG0vbGludXgteDY0LWdudS9pbmRleC5saW51eC14NjQtZ251Lm5vZGVgLFxuICAgICAgXSxcbiAgICBdLFxuICB9KVxuXG4gIHN0YXRpYyBwYXRocyA9IFtbJ2FydGlmYWN0cyddXVxuXG4gIGFzeW5jIGV4ZWN1dGUoKSB7XG4gICAgYXdhaXQgY29sbGVjdEFydGlmYWN0cyh0aGlzLmdldE9wdGlvbnMoKSlcbiAgfVxufVxuIiwiLy8gVGhpcyBmaWxlIGlzIGdlbmVyYXRlZCBieSBjb2RlZ2VuL2luZGV4LnRzXG4vLyBEbyBub3QgZWRpdCB0aGlzIGZpbGUgbWFudWFsbHlcbmltcG9ydCB7IENvbW1hbmQsIE9wdGlvbiB9IGZyb20gJ2NsaXBhbmlvbidcblxuZXhwb3J0IGFic3RyYWN0IGNsYXNzIEJhc2VCdWlsZENvbW1hbmQgZXh0ZW5kcyBDb21tYW5kIHtcbiAgc3RhdGljIHBhdGhzID0gW1snYnVpbGQnXV1cblxuICBzdGF0aWMgdXNhZ2UgPSBDb21tYW5kLlVzYWdlKHtcbiAgICBkZXNjcmlwdGlvbjogJ0J1aWxkIHRoZSBOQVBJLVJTIHByb2plY3QnLFxuICB9KVxuXG4gIHRhcmdldD86IHN0cmluZyA9IE9wdGlvbi5TdHJpbmcoJy0tdGFyZ2V0LC10Jywge1xuICAgIGRlc2NyaXB0aW9uOlxuICAgICAgJ0J1aWxkIGZvciB0aGUgdGFyZ2V0IHRyaXBsZSwgYnlwYXNzZWQgdG8gYGNhcmdvIGJ1aWxkIC0tdGFyZ2V0YCcsXG4gIH0pXG5cbiAgY3dkPzogc3RyaW5nID0gT3B0aW9uLlN0cmluZygnLS1jd2QnLCB7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnVGhlIHdvcmtpbmcgZGlyZWN0b3J5IG9mIHdoZXJlIG5hcGkgY29tbWFuZCB3aWxsIGJlIGV4ZWN1dGVkIGluLCBhbGwgb3RoZXIgcGF0aHMgb3B0aW9ucyBhcmUgcmVsYXRpdmUgdG8gdGhpcyBwYXRoJyxcbiAgfSlcblxuICBtYW5pZmVzdFBhdGg/OiBzdHJpbmcgPSBPcHRpb24uU3RyaW5nKCctLW1hbmlmZXN0LXBhdGgnLCB7XG4gICAgZGVzY3JpcHRpb246ICdQYXRoIHRvIGBDYXJnby50b21sYCcsXG4gIH0pXG5cbiAgY29uZmlnUGF0aD86IHN0cmluZyA9IE9wdGlvbi5TdHJpbmcoJy0tY29uZmlnLXBhdGgsLWMnLCB7XG4gICAgZGVzY3JpcHRpb246ICdQYXRoIHRvIGBuYXBpYCBjb25maWcganNvbiBmaWxlJyxcbiAgfSlcblxuICBwYWNrYWdlSnNvblBhdGg/OiBzdHJpbmcgPSBPcHRpb24uU3RyaW5nKCctLXBhY2thZ2UtanNvbi1wYXRoJywge1xuICAgIGRlc2NyaXB0aW9uOiAnUGF0aCB0byBgcGFja2FnZS5qc29uYCcsXG4gIH0pXG5cbiAgdGFyZ2V0RGlyPzogc3RyaW5nID0gT3B0aW9uLlN0cmluZygnLS10YXJnZXQtZGlyJywge1xuICAgIGRlc2NyaXB0aW9uOlxuICAgICAgJ0RpcmVjdG9yeSBmb3IgYWxsIGNyYXRlIGdlbmVyYXRlZCBhcnRpZmFjdHMsIHNlZSBgY2FyZ28gYnVpbGQgLS10YXJnZXQtZGlyYCcsXG4gIH0pXG5cbiAgb3V0cHV0RGlyPzogc3RyaW5nID0gT3B0aW9uLlN0cmluZygnLS1vdXRwdXQtZGlyLC1vJywge1xuICAgIGRlc2NyaXB0aW9uOlxuICAgICAgJ1BhdGggdG8gd2hlcmUgYWxsIHRoZSBidWlsdCBmaWxlcyB3b3VsZCBiZSBwdXQuIERlZmF1bHQgdG8gdGhlIGNyYXRlIGZvbGRlcicsXG4gIH0pXG5cbiAgcGxhdGZvcm0/OiBib29sZWFuID0gT3B0aW9uLkJvb2xlYW4oJy0tcGxhdGZvcm0nLCB7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnQWRkIHBsYXRmb3JtIHRyaXBsZSB0byB0aGUgZ2VuZXJhdGVkIG5vZGVqcyBiaW5kaW5nIGZpbGUsIGVnOiBgW25hbWVdLmxpbnV4LXg2NC1nbnUubm9kZWAnLFxuICB9KVxuXG4gIGpzUGFja2FnZU5hbWU/OiBzdHJpbmcgPSBPcHRpb24uU3RyaW5nKCctLWpzLXBhY2thZ2UtbmFtZScsIHtcbiAgICBkZXNjcmlwdGlvbjpcbiAgICAgICdQYWNrYWdlIG5hbWUgaW4gZ2VuZXJhdGVkIGpzIGJpbmRpbmcgZmlsZS4gT25seSB3b3JrcyB3aXRoIGAtLXBsYXRmb3JtYCBmbGFnJyxcbiAgfSlcblxuICBjb25zdEVudW0/OiBib29sZWFuID0gT3B0aW9uLkJvb2xlYW4oJy0tY29uc3QtZW51bScsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1doZXRoZXIgZ2VuZXJhdGUgY29uc3QgZW51bSBmb3IgdHlwZXNjcmlwdCBiaW5kaW5ncycsXG4gIH0pXG5cbiAganNCaW5kaW5nPzogc3RyaW5nID0gT3B0aW9uLlN0cmluZygnLS1qcycsIHtcbiAgICBkZXNjcmlwdGlvbjpcbiAgICAgICdQYXRoIGFuZCBmaWxlbmFtZSBvZiBnZW5lcmF0ZWQgSlMgYmluZGluZyBmaWxlLiBPbmx5IHdvcmtzIHdpdGggYC0tcGxhdGZvcm1gIGZsYWcuIFJlbGF0aXZlIHRvIGAtLW91dHB1dC1kaXJgLicsXG4gIH0pXG5cbiAgbm9Kc0JpbmRpbmc/OiBib29sZWFuID0gT3B0aW9uLkJvb2xlYW4oJy0tbm8tanMnLCB7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnV2hldGhlciB0byBkaXNhYmxlIHRoZSBnZW5lcmF0aW9uIEpTIGJpbmRpbmcgZmlsZS4gT25seSB3b3JrcyB3aXRoIGAtLXBsYXRmb3JtYCBmbGFnLicsXG4gIH0pXG5cbiAgZHRzPzogc3RyaW5nID0gT3B0aW9uLlN0cmluZygnLS1kdHMnLCB7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnUGF0aCBhbmQgZmlsZW5hbWUgb2YgZ2VuZXJhdGVkIHR5cGUgZGVmIGZpbGUuIFJlbGF0aXZlIHRvIGAtLW91dHB1dC1kaXJgJyxcbiAgfSlcblxuICBkdHNIZWFkZXI/OiBzdHJpbmcgPSBPcHRpb24uU3RyaW5nKCctLWR0cy1oZWFkZXInLCB7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnQ3VzdG9tIGZpbGUgaGVhZGVyIGZvciBnZW5lcmF0ZWQgdHlwZSBkZWYgZmlsZS4gT25seSB3b3JrcyB3aGVuIGB0eXBlZGVmYCBmZWF0dXJlIGVuYWJsZWQuJyxcbiAgfSlcblxuICBub0R0c0hlYWRlcj86IGJvb2xlYW4gPSBPcHRpb24uQm9vbGVhbignLS1uby1kdHMtaGVhZGVyJywge1xuICAgIGRlc2NyaXB0aW9uOlxuICAgICAgJ1doZXRoZXIgdG8gZGlzYWJsZSB0aGUgZGVmYXVsdCBmaWxlIGhlYWRlciBmb3IgZ2VuZXJhdGVkIHR5cGUgZGVmIGZpbGUuIE9ubHkgd29ya3Mgd2hlbiBgdHlwZWRlZmAgZmVhdHVyZSBlbmFibGVkLicsXG4gIH0pXG5cbiAgZHRzQ2FjaGUgPSBPcHRpb24uQm9vbGVhbignLS1kdHMtY2FjaGUnLCB0cnVlLCB7XG4gICAgZGVzY3JpcHRpb246ICdXaGV0aGVyIHRvIGVuYWJsZSB0aGUgZHRzIGNhY2hlLCBkZWZhdWx0IHRvIHRydWUnLFxuICB9KVxuXG4gIGVzbT86IGJvb2xlYW4gPSBPcHRpb24uQm9vbGVhbignLS1lc20nLCB7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnV2hldGhlciB0byBlbWl0IGFuIEVTTSBKUyBiaW5kaW5nIGZpbGUgaW5zdGVhZCBvZiBDSlMgZm9ybWF0LiBPbmx5IHdvcmtzIHdpdGggYC0tcGxhdGZvcm1gIGZsYWcuJyxcbiAgfSlcblxuICBzdHJpcD86IGJvb2xlYW4gPSBPcHRpb24uQm9vbGVhbignLS1zdHJpcCwtcycsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1doZXRoZXIgc3RyaXAgdGhlIGxpYnJhcnkgdG8gYWNoaWV2ZSB0aGUgbWluaW11bSBmaWxlIHNpemUnLFxuICB9KVxuXG4gIHJlbGVhc2U/OiBib29sZWFuID0gT3B0aW9uLkJvb2xlYW4oJy0tcmVsZWFzZSwtcicsIHtcbiAgICBkZXNjcmlwdGlvbjogJ0J1aWxkIGluIHJlbGVhc2UgbW9kZScsXG4gIH0pXG5cbiAgdmVyYm9zZT86IGJvb2xlYW4gPSBPcHRpb24uQm9vbGVhbignLS12ZXJib3NlLC12Jywge1xuICAgIGRlc2NyaXB0aW9uOiAnVmVyYm9zZWx5IGxvZyBidWlsZCBjb21tYW5kIHRyYWNlJyxcbiAgfSlcblxuICBiaW4/OiBzdHJpbmcgPSBPcHRpb24uU3RyaW5nKCctLWJpbicsIHtcbiAgICBkZXNjcmlwdGlvbjogJ0J1aWxkIG9ubHkgdGhlIHNwZWNpZmllZCBiaW5hcnknLFxuICB9KVxuXG4gIHBhY2thZ2U/OiBzdHJpbmcgPSBPcHRpb24uU3RyaW5nKCctLXBhY2thZ2UsLXAnLCB7XG4gICAgZGVzY3JpcHRpb246ICdCdWlsZCB0aGUgc3BlY2lmaWVkIGxpYnJhcnkgb3IgdGhlIG9uZSBhdCBjd2QnLFxuICB9KVxuXG4gIHByb2ZpbGU/OiBzdHJpbmcgPSBPcHRpb24uU3RyaW5nKCctLXByb2ZpbGUnLCB7XG4gICAgZGVzY3JpcHRpb246ICdCdWlsZCBhcnRpZmFjdHMgd2l0aCB0aGUgc3BlY2lmaWVkIHByb2ZpbGUnLFxuICB9KVxuXG4gIGNyb3NzQ29tcGlsZT86IGJvb2xlYW4gPSBPcHRpb24uQm9vbGVhbignLS1jcm9zcy1jb21waWxlLC14Jywge1xuICAgIGRlc2NyaXB0aW9uOlxuICAgICAgJ1tleHBlcmltZW50YWxdIGNyb3NzLWNvbXBpbGUgZm9yIHRoZSBzcGVjaWZpZWQgdGFyZ2V0IHdpdGggYGNhcmdvLXh3aW5gIG9uIHdpbmRvd3MgYW5kIGBjYXJnby16aWdidWlsZGAgb24gb3RoZXIgcGxhdGZvcm0nLFxuICB9KVxuXG4gIHVzZUNyb3NzPzogYm9vbGVhbiA9IE9wdGlvbi5Cb29sZWFuKCctLXVzZS1jcm9zcycsIHtcbiAgICBkZXNjcmlwdGlvbjpcbiAgICAgICdbZXhwZXJpbWVudGFsXSB1c2UgW2Nyb3NzXShodHRwczovL2dpdGh1Yi5jb20vY3Jvc3MtcnMvY3Jvc3MpIGluc3RlYWQgb2YgYGNhcmdvYCcsXG4gIH0pXG5cbiAgdXNlTmFwaUNyb3NzPzogYm9vbGVhbiA9IE9wdGlvbi5Cb29sZWFuKCctLXVzZS1uYXBpLWNyb3NzJywge1xuICAgIGRlc2NyaXB0aW9uOlxuICAgICAgJ1tleHBlcmltZW50YWxdIHVzZSBAbmFwaS1ycy9jcm9zcy10b29sY2hhaW4gdG8gY3Jvc3MtY29tcGlsZSBMaW51eCBhcm0vYXJtNjQveDY0IGdudSB0YXJnZXRzLicsXG4gIH0pXG5cbiAgd2F0Y2g/OiBib29sZWFuID0gT3B0aW9uLkJvb2xlYW4oJy0td2F0Y2gsLXcnLCB7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnd2F0Y2ggdGhlIGNyYXRlIGNoYW5nZXMgYW5kIGJ1aWxkIGNvbnRpbnVvdXNseSB3aXRoIGBjYXJnby13YXRjaGAgY3JhdGVzJyxcbiAgfSlcblxuICBmZWF0dXJlcz86IHN0cmluZ1tdID0gT3B0aW9uLkFycmF5KCctLWZlYXR1cmVzLC1GJywge1xuICAgIGRlc2NyaXB0aW9uOiAnU3BhY2Utc2VwYXJhdGVkIGxpc3Qgb2YgZmVhdHVyZXMgdG8gYWN0aXZhdGUnLFxuICB9KVxuXG4gIGFsbEZlYXR1cmVzPzogYm9vbGVhbiA9IE9wdGlvbi5Cb29sZWFuKCctLWFsbC1mZWF0dXJlcycsIHtcbiAgICBkZXNjcmlwdGlvbjogJ0FjdGl2YXRlIGFsbCBhdmFpbGFibGUgZmVhdHVyZXMnLFxuICB9KVxuXG4gIG5vRGVmYXVsdEZlYXR1cmVzPzogYm9vbGVhbiA9IE9wdGlvbi5Cb29sZWFuKCctLW5vLWRlZmF1bHQtZmVhdHVyZXMnLCB7XG4gICAgZGVzY3JpcHRpb246ICdEbyBub3QgYWN0aXZhdGUgdGhlIGBkZWZhdWx0YCBmZWF0dXJlJyxcbiAgfSlcblxuICBnZXRPcHRpb25zKCkge1xuICAgIHJldHVybiB7XG4gICAgICB0YXJnZXQ6IHRoaXMudGFyZ2V0LFxuICAgICAgY3dkOiB0aGlzLmN3ZCxcbiAgICAgIG1hbmlmZXN0UGF0aDogdGhpcy5tYW5pZmVzdFBhdGgsXG4gICAgICBjb25maWdQYXRoOiB0aGlzLmNvbmZpZ1BhdGgsXG4gICAgICBwYWNrYWdlSnNvblBhdGg6IHRoaXMucGFja2FnZUpzb25QYXRoLFxuICAgICAgdGFyZ2V0RGlyOiB0aGlzLnRhcmdldERpcixcbiAgICAgIG91dHB1dERpcjogdGhpcy5vdXRwdXREaXIsXG4gICAgICBwbGF0Zm9ybTogdGhpcy5wbGF0Zm9ybSxcbiAgICAgIGpzUGFja2FnZU5hbWU6IHRoaXMuanNQYWNrYWdlTmFtZSxcbiAgICAgIGNvbnN0RW51bTogdGhpcy5jb25zdEVudW0sXG4gICAgICBqc0JpbmRpbmc6IHRoaXMuanNCaW5kaW5nLFxuICAgICAgbm9Kc0JpbmRpbmc6IHRoaXMubm9Kc0JpbmRpbmcsXG4gICAgICBkdHM6IHRoaXMuZHRzLFxuICAgICAgZHRzSGVhZGVyOiB0aGlzLmR0c0hlYWRlcixcbiAgICAgIG5vRHRzSGVhZGVyOiB0aGlzLm5vRHRzSGVhZGVyLFxuICAgICAgZHRzQ2FjaGU6IHRoaXMuZHRzQ2FjaGUsXG4gICAgICBlc206IHRoaXMuZXNtLFxuICAgICAgc3RyaXA6IHRoaXMuc3RyaXAsXG4gICAgICByZWxlYXNlOiB0aGlzLnJlbGVhc2UsXG4gICAgICB2ZXJib3NlOiB0aGlzLnZlcmJvc2UsXG4gICAgICBiaW46IHRoaXMuYmluLFxuICAgICAgcGFja2FnZTogdGhpcy5wYWNrYWdlLFxuICAgICAgcHJvZmlsZTogdGhpcy5wcm9maWxlLFxuICAgICAgY3Jvc3NDb21waWxlOiB0aGlzLmNyb3NzQ29tcGlsZSxcbiAgICAgIHVzZUNyb3NzOiB0aGlzLnVzZUNyb3NzLFxuICAgICAgdXNlTmFwaUNyb3NzOiB0aGlzLnVzZU5hcGlDcm9zcyxcbiAgICAgIHdhdGNoOiB0aGlzLndhdGNoLFxuICAgICAgZmVhdHVyZXM6IHRoaXMuZmVhdHVyZXMsXG4gICAgICBhbGxGZWF0dXJlczogdGhpcy5hbGxGZWF0dXJlcyxcbiAgICAgIG5vRGVmYXVsdEZlYXR1cmVzOiB0aGlzLm5vRGVmYXVsdEZlYXR1cmVzLFxuICAgIH1cbiAgfVxufVxuXG4vKipcbiAqIEJ1aWxkIHRoZSBOQVBJLVJTIHByb2plY3RcbiAqL1xuZXhwb3J0IGludGVyZmFjZSBCdWlsZE9wdGlvbnMge1xuICAvKipcbiAgICogQnVpbGQgZm9yIHRoZSB0YXJnZXQgdHJpcGxlLCBieXBhc3NlZCB0byBgY2FyZ28gYnVpbGQgLS10YXJnZXRgXG4gICAqL1xuICB0YXJnZXQ/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFRoZSB3b3JraW5nIGRpcmVjdG9yeSBvZiB3aGVyZSBuYXBpIGNvbW1hbmQgd2lsbCBiZSBleGVjdXRlZCBpbiwgYWxsIG90aGVyIHBhdGhzIG9wdGlvbnMgYXJlIHJlbGF0aXZlIHRvIHRoaXMgcGF0aFxuICAgKi9cbiAgY3dkPzogc3RyaW5nXG4gIC8qKlxuICAgKiBQYXRoIHRvIGBDYXJnby50b21sYFxuICAgKi9cbiAgbWFuaWZlc3RQYXRoPzogc3RyaW5nXG4gIC8qKlxuICAgKiBQYXRoIHRvIGBuYXBpYCBjb25maWcganNvbiBmaWxlXG4gICAqL1xuICBjb25maWdQYXRoPzogc3RyaW5nXG4gIC8qKlxuICAgKiBQYXRoIHRvIGBwYWNrYWdlLmpzb25gXG4gICAqL1xuICBwYWNrYWdlSnNvblBhdGg/OiBzdHJpbmdcbiAgLyoqXG4gICAqIERpcmVjdG9yeSBmb3IgYWxsIGNyYXRlIGdlbmVyYXRlZCBhcnRpZmFjdHMsIHNlZSBgY2FyZ28gYnVpbGQgLS10YXJnZXQtZGlyYFxuICAgKi9cbiAgdGFyZ2V0RGlyPzogc3RyaW5nXG4gIC8qKlxuICAgKiBQYXRoIHRvIHdoZXJlIGFsbCB0aGUgYnVpbHQgZmlsZXMgd291bGQgYmUgcHV0LiBEZWZhdWx0IHRvIHRoZSBjcmF0ZSBmb2xkZXJcbiAgICovXG4gIG91dHB1dERpcj86IHN0cmluZ1xuICAvKipcbiAgICogQWRkIHBsYXRmb3JtIHRyaXBsZSB0byB0aGUgZ2VuZXJhdGVkIG5vZGVqcyBiaW5kaW5nIGZpbGUsIGVnOiBgW25hbWVdLmxpbnV4LXg2NC1nbnUubm9kZWBcbiAgICovXG4gIHBsYXRmb3JtPzogYm9vbGVhblxuICAvKipcbiAgICogUGFja2FnZSBuYW1lIGluIGdlbmVyYXRlZCBqcyBiaW5kaW5nIGZpbGUuIE9ubHkgd29ya3Mgd2l0aCBgLS1wbGF0Zm9ybWAgZmxhZ1xuICAgKi9cbiAganNQYWNrYWdlTmFtZT86IHN0cmluZ1xuICAvKipcbiAgICogV2hldGhlciBnZW5lcmF0ZSBjb25zdCBlbnVtIGZvciB0eXBlc2NyaXB0IGJpbmRpbmdzXG4gICAqL1xuICBjb25zdEVudW0/OiBib29sZWFuXG4gIC8qKlxuICAgKiBQYXRoIGFuZCBmaWxlbmFtZSBvZiBnZW5lcmF0ZWQgSlMgYmluZGluZyBmaWxlLiBPbmx5IHdvcmtzIHdpdGggYC0tcGxhdGZvcm1gIGZsYWcuIFJlbGF0aXZlIHRvIGAtLW91dHB1dC1kaXJgLlxuICAgKi9cbiAganNCaW5kaW5nPzogc3RyaW5nXG4gIC8qKlxuICAgKiBXaGV0aGVyIHRvIGRpc2FibGUgdGhlIGdlbmVyYXRpb24gSlMgYmluZGluZyBmaWxlLiBPbmx5IHdvcmtzIHdpdGggYC0tcGxhdGZvcm1gIGZsYWcuXG4gICAqL1xuICBub0pzQmluZGluZz86IGJvb2xlYW5cbiAgLyoqXG4gICAqIFBhdGggYW5kIGZpbGVuYW1lIG9mIGdlbmVyYXRlZCB0eXBlIGRlZiBmaWxlLiBSZWxhdGl2ZSB0byBgLS1vdXRwdXQtZGlyYFxuICAgKi9cbiAgZHRzPzogc3RyaW5nXG4gIC8qKlxuICAgKiBDdXN0b20gZmlsZSBoZWFkZXIgZm9yIGdlbmVyYXRlZCB0eXBlIGRlZiBmaWxlLiBPbmx5IHdvcmtzIHdoZW4gYHR5cGVkZWZgIGZlYXR1cmUgZW5hYmxlZC5cbiAgICovXG4gIGR0c0hlYWRlcj86IHN0cmluZ1xuICAvKipcbiAgICogV2hldGhlciB0byBkaXNhYmxlIHRoZSBkZWZhdWx0IGZpbGUgaGVhZGVyIGZvciBnZW5lcmF0ZWQgdHlwZSBkZWYgZmlsZS4gT25seSB3b3JrcyB3aGVuIGB0eXBlZGVmYCBmZWF0dXJlIGVuYWJsZWQuXG4gICAqL1xuICBub0R0c0hlYWRlcj86IGJvb2xlYW5cbiAgLyoqXG4gICAqIFdoZXRoZXIgdG8gZW5hYmxlIHRoZSBkdHMgY2FjaGUsIGRlZmF1bHQgdG8gdHJ1ZVxuICAgKlxuICAgKiBAZGVmYXVsdCB0cnVlXG4gICAqL1xuICBkdHNDYWNoZT86IGJvb2xlYW5cbiAgLyoqXG4gICAqIFdoZXRoZXIgdG8gZW1pdCBhbiBFU00gSlMgYmluZGluZyBmaWxlIGluc3RlYWQgb2YgQ0pTIGZvcm1hdC4gT25seSB3b3JrcyB3aXRoIGAtLXBsYXRmb3JtYCBmbGFnLlxuICAgKi9cbiAgZXNtPzogYm9vbGVhblxuICAvKipcbiAgICogV2hldGhlciBzdHJpcCB0aGUgbGlicmFyeSB0byBhY2hpZXZlIHRoZSBtaW5pbXVtIGZpbGUgc2l6ZVxuICAgKi9cbiAgc3RyaXA/OiBib29sZWFuXG4gIC8qKlxuICAgKiBCdWlsZCBpbiByZWxlYXNlIG1vZGVcbiAgICovXG4gIHJlbGVhc2U/OiBib29sZWFuXG4gIC8qKlxuICAgKiBWZXJib3NlbHkgbG9nIGJ1aWxkIGNvbW1hbmQgdHJhY2VcbiAgICovXG4gIHZlcmJvc2U/OiBib29sZWFuXG4gIC8qKlxuICAgKiBCdWlsZCBvbmx5IHRoZSBzcGVjaWZpZWQgYmluYXJ5XG4gICAqL1xuICBiaW4/OiBzdHJpbmdcbiAgLyoqXG4gICAqIEJ1aWxkIHRoZSBzcGVjaWZpZWQgbGlicmFyeSBvciB0aGUgb25lIGF0IGN3ZFxuICAgKi9cbiAgcGFja2FnZT86IHN0cmluZ1xuICAvKipcbiAgICogQnVpbGQgYXJ0aWZhY3RzIHdpdGggdGhlIHNwZWNpZmllZCBwcm9maWxlXG4gICAqL1xuICBwcm9maWxlPzogc3RyaW5nXG4gIC8qKlxuICAgKiBbZXhwZXJpbWVudGFsXSBjcm9zcy1jb21waWxlIGZvciB0aGUgc3BlY2lmaWVkIHRhcmdldCB3aXRoIGBjYXJnby14d2luYCBvbiB3aW5kb3dzIGFuZCBgY2FyZ28temlnYnVpbGRgIG9uIG90aGVyIHBsYXRmb3JtXG4gICAqL1xuICBjcm9zc0NvbXBpbGU/OiBib29sZWFuXG4gIC8qKlxuICAgKiBbZXhwZXJpbWVudGFsXSB1c2UgW2Nyb3NzXShodHRwczovL2dpdGh1Yi5jb20vY3Jvc3MtcnMvY3Jvc3MpIGluc3RlYWQgb2YgYGNhcmdvYFxuICAgKi9cbiAgdXNlQ3Jvc3M/OiBib29sZWFuXG4gIC8qKlxuICAgKiBbZXhwZXJpbWVudGFsXSB1c2UgQG5hcGktcnMvY3Jvc3MtdG9vbGNoYWluIHRvIGNyb3NzLWNvbXBpbGUgTGludXggYXJtL2FybTY0L3g2NCBnbnUgdGFyZ2V0cy5cbiAgICovXG4gIHVzZU5hcGlDcm9zcz86IGJvb2xlYW5cbiAgLyoqXG4gICAqIHdhdGNoIHRoZSBjcmF0ZSBjaGFuZ2VzIGFuZCBidWlsZCBjb250aW51b3VzbHkgd2l0aCBgY2FyZ28td2F0Y2hgIGNyYXRlc1xuICAgKi9cbiAgd2F0Y2g/OiBib29sZWFuXG4gIC8qKlxuICAgKiBTcGFjZS1zZXBhcmF0ZWQgbGlzdCBvZiBmZWF0dXJlcyB0byBhY3RpdmF0ZVxuICAgKi9cbiAgZmVhdHVyZXM/OiBzdHJpbmdbXVxuICAvKipcbiAgICogQWN0aXZhdGUgYWxsIGF2YWlsYWJsZSBmZWF0dXJlc1xuICAgKi9cbiAgYWxsRmVhdHVyZXM/OiBib29sZWFuXG4gIC8qKlxuICAgKiBEbyBub3QgYWN0aXZhdGUgdGhlIGBkZWZhdWx0YCBmZWF0dXJlXG4gICAqL1xuICBub0RlZmF1bHRGZWF0dXJlcz86IGJvb2xlYW5cbn1cblxuZXhwb3J0IGZ1bmN0aW9uIGFwcGx5RGVmYXVsdEJ1aWxkT3B0aW9ucyhvcHRpb25zOiBCdWlsZE9wdGlvbnMpIHtcbiAgcmV0dXJuIHtcbiAgICBkdHNDYWNoZTogdHJ1ZSxcbiAgICAuLi5vcHRpb25zLFxuICB9XG59XG4iLCJpbXBvcnQgeyBleGVjU3luYyB9IGZyb20gJ25vZGU6Y2hpbGRfcHJvY2VzcydcblxuaW1wb3J0IHsgT3B0aW9uIH0gZnJvbSAnY2xpcGFuaW9uJ1xuXG5pbXBvcnQgeyBidWlsZFByb2plY3QgfSBmcm9tICcuLi9hcGkvYnVpbGQuanMnXG5pbXBvcnQgeyBCYXNlQnVpbGRDb21tYW5kIH0gZnJvbSAnLi4vZGVmL2J1aWxkLmpzJ1xuaW1wb3J0IHsgZGVidWdGYWN0b3J5IH0gZnJvbSAnLi4vdXRpbHMvaW5kZXguanMnXG5cbmNvbnN0IGRlYnVnID0gZGVidWdGYWN0b3J5KCdidWlsZCcpXG5cbmV4cG9ydCBjbGFzcyBCdWlsZENvbW1hbmQgZXh0ZW5kcyBCYXNlQnVpbGRDb21tYW5kIHtcbiAgcGlwZSA9IE9wdGlvbi5TdHJpbmcoJy0tcGlwZScsIHtcbiAgICBkZXNjcmlwdGlvbjpcbiAgICAgICdQaXBlIGFsbCBvdXRwdXRzIGZpbGUgdG8gZ2l2ZW4gY29tbWFuZC4gZS5nLiBgbmFwaSBidWlsZCAtLXBpcGUgXCJucHggcHJldHRpZXIgLS13cml0ZVwiYCcsXG4gIH0pXG5cbiAgY2FyZ29PcHRpb25zID0gT3B0aW9uLlJlc3QoKVxuXG4gIGFzeW5jIGV4ZWN1dGUoKSB7XG4gICAgY29uc3QgeyB0YXNrIH0gPSBhd2FpdCBidWlsZFByb2plY3Qoe1xuICAgICAgLi4udGhpcy5nZXRPcHRpb25zKCksXG4gICAgICBjYXJnb09wdGlvbnM6IHRoaXMuY2FyZ29PcHRpb25zLFxuICAgIH0pXG5cbiAgICBjb25zdCBvdXRwdXRzID0gYXdhaXQgdGFza1xuXG4gICAgaWYgKHRoaXMucGlwZSkge1xuICAgICAgZm9yIChjb25zdCBvdXRwdXQgb2Ygb3V0cHV0cykge1xuICAgICAgICBkZWJ1ZygnUGlwaW5nIG91dHB1dCBmaWxlIHRvIGNvbW1hbmQ6ICVzJywgdGhpcy5waXBlKVxuICAgICAgICB0cnkge1xuICAgICAgICAgIGV4ZWNTeW5jKGAke3RoaXMucGlwZX0gJHtvdXRwdXQucGF0aH1gLCB7XG4gICAgICAgICAgICBzdGRpbzogJ2luaGVyaXQnLFxuICAgICAgICAgICAgY3dkOiB0aGlzLmN3ZCxcbiAgICAgICAgICB9KVxuICAgICAgICB9IGNhdGNoIChlKSB7XG4gICAgICAgICAgZGVidWcuZXJyb3IoYEZhaWxlZCB0byBwaXBlIG91dHB1dCBmaWxlICR7b3V0cHV0LnBhdGh9IHRvIGNvbW1hbmRgKVxuICAgICAgICAgIGRlYnVnLmVycm9yKGUpXG4gICAgICAgIH1cbiAgICAgIH1cbiAgICB9XG4gIH1cbn1cbiIsImltcG9ydCB7IGNyZWF0ZU5wbURpcnMgfSBmcm9tICcuLi9hcGkvY3JlYXRlLW5wbS1kaXJzLmpzJ1xuaW1wb3J0IHsgQmFzZUNyZWF0ZU5wbURpcnNDb21tYW5kIH0gZnJvbSAnLi4vZGVmL2NyZWF0ZS1ucG0tZGlycy5qcydcblxuZXhwb3J0IGNsYXNzIENyZWF0ZU5wbURpcnNDb21tYW5kIGV4dGVuZHMgQmFzZUNyZWF0ZU5wbURpcnNDb21tYW5kIHtcbiAgYXN5bmMgZXhlY3V0ZSgpIHtcbiAgICBhd2FpdCBjcmVhdGVOcG1EaXJzKHRoaXMuZ2V0T3B0aW9ucygpKVxuICB9XG59XG4iLCJpbXBvcnQgeyBDb21tYW5kIH0gZnJvbSAnY2xpcGFuaW9uJ1xuXG4vKipcbiAqIEEgY29tbWFuZCB0aGF0IHByaW50cyB0aGUgdXNhZ2Ugb2YgYWxsIGNvbW1hbmRzLlxuICpcbiAqIFBhdGhzOiBgLWhgLCBgLS1oZWxwYFxuICovXG5leHBvcnQgY2xhc3MgSGVscENvbW1hbmQgZXh0ZW5kcyBDb21tYW5kPGFueT4ge1xuICBzdGF0aWMgcGF0aHMgPSBbW2AtaGBdLCBbYC0taGVscGBdXVxuICBhc3luYyBleGVjdXRlKCkge1xuICAgIGF3YWl0IHRoaXMuY29udGV4dC5zdGRvdXQud3JpdGUodGhpcy5jbGkudXNhZ2UoKSlcbiAgfVxufVxuIiwiaW1wb3J0IHBhdGggZnJvbSAnbm9kZTpwYXRoJ1xuXG5pbXBvcnQgeyBpbnB1dCwgc2VsZWN0LCBjaGVja2JveCwgY29uZmlybSB9IGZyb20gJ0BpbnF1aXJlci9wcm9tcHRzJ1xuaW1wb3J0IHsgT3B0aW9uIH0gZnJvbSAnY2xpcGFuaW9uJ1xuXG5pbXBvcnQgeyBuZXdQcm9qZWN0IH0gZnJvbSAnLi4vYXBpL25ldy5qcydcbmltcG9ydCB7IEJhc2VOZXdDb21tYW5kIH0gZnJvbSAnLi4vZGVmL25ldy5qcydcbmltcG9ydCB7XG4gIEFWQUlMQUJMRV9UQVJHRVRTLFxuICBkZWJ1Z0ZhY3RvcnksXG4gIERFRkFVTFRfVEFSR0VUUyxcbiAgdHlwZSBUYXJnZXRUcmlwbGUsXG59IGZyb20gJy4uL3V0aWxzL2luZGV4LmpzJ1xuaW1wb3J0IHsgbmFwaUVuZ2luZVJlcXVpcmVtZW50IH0gZnJvbSAnLi4vdXRpbHMvdmVyc2lvbi5qcydcblxuY29uc3QgZGVidWcgPSBkZWJ1Z0ZhY3RvcnkoJ25ldycpXG5cbmV4cG9ydCBjbGFzcyBOZXdDb21tYW5kIGV4dGVuZHMgQmFzZU5ld0NvbW1hbmQge1xuICBpbnRlcmFjdGl2ZSA9IE9wdGlvbi5Cb29sZWFuKCctLWludGVyYWN0aXZlLC1pJywgdHJ1ZSwge1xuICAgIGRlc2NyaXB0aW9uOlxuICAgICAgJ0FzayBwcm9qZWN0IGJhc2ljIGluZm9ybWF0aW9uIGludGVyYWN0aXZlbHkgd2l0aG91dCBqdXN0IHVzaW5nIHRoZSBkZWZhdWx0LicsXG4gIH0pXG5cbiAgYXN5bmMgZXhlY3V0ZSgpIHtcbiAgICB0cnkge1xuICAgICAgY29uc3Qgb3B0aW9ucyA9IGF3YWl0IHRoaXMuZmV0Y2hPcHRpb25zKClcbiAgICAgIGF3YWl0IG5ld1Byb2plY3Qob3B0aW9ucylcbiAgICAgIHJldHVybiAwXG4gICAgfSBjYXRjaCAoZSkge1xuICAgICAgZGVidWcoJ0ZhaWxlZCB0byBjcmVhdGUgbmV3IHByb2plY3QnKVxuICAgICAgZGVidWcuZXJyb3IoZSlcbiAgICAgIHJldHVybiAxXG4gICAgfVxuICB9XG5cbiAgcHJpdmF0ZSBhc3luYyBmZXRjaE9wdGlvbnMoKSB7XG4gICAgY29uc3QgY21kT3B0aW9ucyA9IHN1cGVyLmdldE9wdGlvbnMoKVxuXG4gICAgaWYgKHRoaXMuaW50ZXJhY3RpdmUpIHtcbiAgICAgIGNvbnN0IHRhcmdldFBhdGg6IHN0cmluZyA9IGNtZE9wdGlvbnMucGF0aFxuICAgICAgICA/IGNtZE9wdGlvbnMucGF0aFxuICAgICAgICA6IGF3YWl0IGlucXVpcmVyUHJvamVjdFBhdGgoKVxuICAgICAgY21kT3B0aW9ucy5wYXRoID0gdGFyZ2V0UGF0aFxuICAgICAgcmV0dXJuIHtcbiAgICAgICAgLi4uY21kT3B0aW9ucyxcbiAgICAgICAgbmFtZTogYXdhaXQgdGhpcy5mZXRjaE5hbWUocGF0aC5wYXJzZSh0YXJnZXRQYXRoKS5iYXNlKSxcbiAgICAgICAgbWluTm9kZUFwaVZlcnNpb246IGF3YWl0IHRoaXMuZmV0Y2hOYXBpVmVyc2lvbigpLFxuICAgICAgICB0YXJnZXRzOiBhd2FpdCB0aGlzLmZldGNoVGFyZ2V0cygpLFxuICAgICAgICBsaWNlbnNlOiBhd2FpdCB0aGlzLmZldGNoTGljZW5zZSgpLFxuICAgICAgICBlbmFibGVUeXBlRGVmOiBhd2FpdCB0aGlzLmZldGNoVHlwZURlZigpLFxuICAgICAgICBlbmFibGVHaXRodWJBY3Rpb25zOiBhd2FpdCB0aGlzLmZldGNoR2l0aHViQWN0aW9ucygpLFxuICAgICAgfVxuICAgIH1cblxuICAgIHJldHVybiBjbWRPcHRpb25zXG4gIH1cblxuICBwcml2YXRlIGFzeW5jIGZldGNoTmFtZShkZWZhdWx0TmFtZTogc3RyaW5nKTogUHJvbWlzZTxzdHJpbmc+IHtcbiAgICByZXR1cm4gKFxuICAgICAgdGhpcy4kJG5hbWUgPz9cbiAgICAgIGlucHV0KHtcbiAgICAgICAgbWVzc2FnZTogJ1BhY2thZ2UgbmFtZSAodGhlIG5hbWUgZmllbGQgaW4geW91ciBwYWNrYWdlLmpzb24gZmlsZSknLFxuICAgICAgICBkZWZhdWx0OiBkZWZhdWx0TmFtZSxcbiAgICAgIH0pXG4gICAgKVxuICB9XG5cbiAgcHJpdmF0ZSBhc3luYyBmZXRjaExpY2Vuc2UoKTogUHJvbWlzZTxzdHJpbmc+IHtcbiAgICByZXR1cm4gaW5wdXQoe1xuICAgICAgbWVzc2FnZTogJ0xpY2Vuc2UgZm9yIG9wZW4tc291cmNlZCBwcm9qZWN0JyxcbiAgICAgIGRlZmF1bHQ6IHRoaXMubGljZW5zZSxcbiAgICB9KVxuICB9XG5cbiAgcHJpdmF0ZSBhc3luYyBmZXRjaE5hcGlWZXJzaW9uKCk6IFByb21pc2U8bnVtYmVyPiB7XG4gICAgcmV0dXJuIHNlbGVjdCh7XG4gICAgICBtZXNzYWdlOiAnTWluaW11bSBub2RlLWFwaSB2ZXJzaW9uICh3aXRoIG5vZGUgdmVyc2lvbiByZXF1aXJlbWVudCknLFxuICAgICAgbG9vcDogZmFsc2UsXG4gICAgICBwYWdlU2l6ZTogMTAsXG4gICAgICBjaG9pY2VzOiBBcnJheS5mcm9tKHsgbGVuZ3RoOiA4IH0sIChfLCBpKSA9PiAoe1xuICAgICAgICBuYW1lOiBgbmFwaSR7aSArIDF9ICgke25hcGlFbmdpbmVSZXF1aXJlbWVudChpICsgMSl9KWAsXG4gICAgICAgIHZhbHVlOiBpICsgMSxcbiAgICAgIH0pKSxcbiAgICAgIC8vIGNob2ljZSBpbmRleFxuICAgICAgZGVmYXVsdDogdGhpcy5taW5Ob2RlQXBpVmVyc2lvbiAtIDEsXG4gICAgfSlcbiAgfVxuXG4gIHByaXZhdGUgYXN5bmMgZmV0Y2hUYXJnZXRzKCk6IFByb21pc2U8VGFyZ2V0VHJpcGxlW10+IHtcbiAgICBpZiAodGhpcy5lbmFibGVBbGxUYXJnZXRzKSB7XG4gICAgICByZXR1cm4gQVZBSUxBQkxFX1RBUkdFVFMuY29uY2F0KClcbiAgICB9XG5cbiAgICBjb25zdCB0YXJnZXRzID0gYXdhaXQgY2hlY2tib3goe1xuICAgICAgbG9vcDogZmFsc2UsXG4gICAgICBtZXNzYWdlOiAnQ2hvb3NlIHRhcmdldChzKSB5b3VyIGNyYXRlIHdpbGwgYmUgY29tcGlsZWQgdG8nLFxuICAgICAgY2hvaWNlczogQVZBSUxBQkxFX1RBUkdFVFMubWFwKCh0YXJnZXQpID0+ICh7XG4gICAgICAgIG5hbWU6IHRhcmdldCxcbiAgICAgICAgdmFsdWU6IHRhcmdldCxcbiAgICAgICAgLy8gQHRzLWV4cGVjdC1lcnJvclxuICAgICAgICBjaGVja2VkOiBERUZBVUxUX1RBUkdFVFMuaW5jbHVkZXModGFyZ2V0KSxcbiAgICAgIH0pKSxcbiAgICB9KVxuXG4gICAgcmV0dXJuIHRhcmdldHNcbiAgfVxuXG4gIHByaXZhdGUgYXN5bmMgZmV0Y2hUeXBlRGVmKCk6IFByb21pc2U8Ym9vbGVhbj4ge1xuICAgIGNvbnN0IGVuYWJsZVR5cGVEZWYgPSBhd2FpdCBjb25maXJtKHtcbiAgICAgIG1lc3NhZ2U6ICdFbmFibGUgdHlwZSBkZWZpbml0aW9uIGF1dG8tZ2VuZXJhdGlvbicsXG4gICAgICBkZWZhdWx0OiB0aGlzLmVuYWJsZVR5cGVEZWYsXG4gICAgfSlcblxuICAgIHJldHVybiBlbmFibGVUeXBlRGVmXG4gIH1cblxuICBwcml2YXRlIGFzeW5jIGZldGNoR2l0aHViQWN0aW9ucygpOiBQcm9taXNlPGJvb2xlYW4+IHtcbiAgICBjb25zdCBlbmFibGVHaXRodWJBY3Rpb25zID0gYXdhaXQgY29uZmlybSh7XG4gICAgICBtZXNzYWdlOiAnRW5hYmxlIEdpdGh1YiBBY3Rpb25zIENJJyxcbiAgICAgIGRlZmF1bHQ6IHRoaXMuZW5hYmxlR2l0aHViQWN0aW9ucyxcbiAgICB9KVxuXG4gICAgcmV0dXJuIGVuYWJsZUdpdGh1YkFjdGlvbnNcbiAgfVxufVxuXG5hc3luYyBmdW5jdGlvbiBpbnF1aXJlclByb2plY3RQYXRoKCk6IFByb21pc2U8c3RyaW5nPiB7XG4gIHJldHVybiBpbnB1dCh7XG4gICAgbWVzc2FnZTogJ1RhcmdldCBwYXRoIHRvIGNyZWF0ZSB0aGUgcHJvamVjdCwgcmVsYXRpdmUgdG8gY3dkLicsXG4gIH0pLnRoZW4oKHBhdGgpID0+IHtcbiAgICBpZiAoIXBhdGgpIHtcbiAgICAgIHJldHVybiBpbnF1aXJlclByb2plY3RQYXRoKClcbiAgICB9XG4gICAgcmV0dXJuIHBhdGhcbiAgfSlcbn1cbiIsImltcG9ydCB7IHByZVB1Ymxpc2ggfSBmcm9tICcuLi9hcGkvcHJlLXB1Ymxpc2guanMnXG5pbXBvcnQgeyBCYXNlUHJlUHVibGlzaENvbW1hbmQgfSBmcm9tICcuLi9kZWYvcHJlLXB1Ymxpc2guanMnXG5cbmV4cG9ydCBjbGFzcyBQcmVQdWJsaXNoQ29tbWFuZCBleHRlbmRzIEJhc2VQcmVQdWJsaXNoQ29tbWFuZCB7XG4gIGFzeW5jIGV4ZWN1dGUoKSB7XG4gICAgLy8gQHRzLWV4cGVjdC1lcnJvciBjb25zdCAnbnBtJyB8ICdsZXJuYScgdG8gc3RyaW5nXG4gICAgYXdhaXQgcHJlUHVibGlzaCh0aGlzLmdldE9wdGlvbnMoKSlcbiAgfVxufVxuIiwiaW1wb3J0IHsgaW5wdXQgfSBmcm9tICdAaW5xdWlyZXIvcHJvbXB0cydcblxuaW1wb3J0IHsgcmVuYW1lUHJvamVjdCB9IGZyb20gJy4uL2FwaS9yZW5hbWUuanMnXG5pbXBvcnQgeyBCYXNlUmVuYW1lQ29tbWFuZCB9IGZyb20gJy4uL2RlZi9yZW5hbWUuanMnXG5cbmV4cG9ydCBjbGFzcyBSZW5hbWVDb21tYW5kIGV4dGVuZHMgQmFzZVJlbmFtZUNvbW1hbmQge1xuICBhc3luYyBleGVjdXRlKCkge1xuICAgIGNvbnN0IG9wdGlvbnMgPSB0aGlzLmdldE9wdGlvbnMoKVxuICAgIGlmICghb3B0aW9ucy5uYW1lKSB7XG4gICAgICBjb25zdCBuYW1lID0gYXdhaXQgaW5wdXQoe1xuICAgICAgICBtZXNzYWdlOiBgRW50ZXIgdGhlIG5ldyBwYWNrYWdlIG5hbWUgaW4gdGhlIHBhY2thZ2UuanNvbmAsXG4gICAgICAgIHJlcXVpcmVkOiB0cnVlLFxuICAgICAgfSlcbiAgICAgIG9wdGlvbnMubmFtZSA9IG5hbWVcbiAgICB9XG4gICAgaWYgKCFvcHRpb25zLmJpbmFyeU5hbWUpIHtcbiAgICAgIGNvbnN0IGJpbmFyeU5hbWUgPSBhd2FpdCBpbnB1dCh7XG4gICAgICAgIG1lc3NhZ2U6IGBFbnRlciB0aGUgbmV3IGJpbmFyeSBuYW1lYCxcbiAgICAgICAgcmVxdWlyZWQ6IHRydWUsXG4gICAgICB9KVxuICAgICAgb3B0aW9ucy5iaW5hcnlOYW1lID0gYmluYXJ5TmFtZVxuICAgIH1cbiAgICBhd2FpdCByZW5hbWVQcm9qZWN0KG9wdGlvbnMpXG4gIH1cbn1cbiIsImltcG9ydCB7IHVuaXZlcnNhbGl6ZUJpbmFyaWVzIH0gZnJvbSAnLi4vYXBpL3VuaXZlcnNhbGl6ZS5qcydcbmltcG9ydCB7IEJhc2VVbml2ZXJzYWxpemVDb21tYW5kIH0gZnJvbSAnLi4vZGVmL3VuaXZlcnNhbGl6ZS5qcydcblxuZXhwb3J0IGNsYXNzIFVuaXZlcnNhbGl6ZUNvbW1hbmQgZXh0ZW5kcyBCYXNlVW5pdmVyc2FsaXplQ29tbWFuZCB7XG4gIGFzeW5jIGV4ZWN1dGUoKSB7XG4gICAgYXdhaXQgdW5pdmVyc2FsaXplQmluYXJpZXModGhpcy5nZXRPcHRpb25zKCkpXG4gIH1cbn1cbiIsImltcG9ydCB7IHZlcnNpb24gfSBmcm9tICcuLi9hcGkvdmVyc2lvbi5qcydcbmltcG9ydCB7IEJhc2VWZXJzaW9uQ29tbWFuZCB9IGZyb20gJy4uL2RlZi92ZXJzaW9uLmpzJ1xuXG5leHBvcnQgY2xhc3MgVmVyc2lvbkNvbW1hbmQgZXh0ZW5kcyBCYXNlVmVyc2lvbkNvbW1hbmQge1xuICBhc3luYyBleGVjdXRlKCkge1xuICAgIGF3YWl0IHZlcnNpb24odGhpcy5nZXRPcHRpb25zKCkpXG4gIH1cbn1cbiIsImltcG9ydCB7IENsaSB9IGZyb20gJ2NsaXBhbmlvbidcblxuaW1wb3J0IHsgY29sbGVjdEFydGlmYWN0cyB9IGZyb20gJy4vYXBpL2FydGlmYWN0cy5qcydcbmltcG9ydCB7IGJ1aWxkUHJvamVjdCB9IGZyb20gJy4vYXBpL2J1aWxkLmpzJ1xuaW1wb3J0IHsgY3JlYXRlTnBtRGlycyB9IGZyb20gJy4vYXBpL2NyZWF0ZS1ucG0tZGlycy5qcydcbmltcG9ydCB7IG5ld1Byb2plY3QgfSBmcm9tICcuL2FwaS9uZXcuanMnXG5pbXBvcnQgeyBwcmVQdWJsaXNoIH0gZnJvbSAnLi9hcGkvcHJlLXB1Ymxpc2guanMnXG5pbXBvcnQgeyByZW5hbWVQcm9qZWN0IH0gZnJvbSAnLi9hcGkvcmVuYW1lLmpzJ1xuaW1wb3J0IHsgdW5pdmVyc2FsaXplQmluYXJpZXMgfSBmcm9tICcuL2FwaS91bml2ZXJzYWxpemUuanMnXG5pbXBvcnQgeyB2ZXJzaW9uIH0gZnJvbSAnLi9hcGkvdmVyc2lvbi5qcydcbmltcG9ydCB7IEFydGlmYWN0c0NvbW1hbmQgfSBmcm9tICcuL2NvbW1hbmRzL2FydGlmYWN0cy5qcydcbmltcG9ydCB7IEJ1aWxkQ29tbWFuZCB9IGZyb20gJy4vY29tbWFuZHMvYnVpbGQuanMnXG5pbXBvcnQgeyBDcmVhdGVOcG1EaXJzQ29tbWFuZCB9IGZyb20gJy4vY29tbWFuZHMvY3JlYXRlLW5wbS1kaXJzLmpzJ1xuaW1wb3J0IHsgSGVscENvbW1hbmQgfSBmcm9tICcuL2NvbW1hbmRzL2hlbHAuanMnXG5pbXBvcnQgeyBOZXdDb21tYW5kIH0gZnJvbSAnLi9jb21tYW5kcy9uZXcuanMnXG5pbXBvcnQgeyBQcmVQdWJsaXNoQ29tbWFuZCB9IGZyb20gJy4vY29tbWFuZHMvcHJlLXB1Ymxpc2guanMnXG5pbXBvcnQgeyBSZW5hbWVDb21tYW5kIH0gZnJvbSAnLi9jb21tYW5kcy9yZW5hbWUuanMnXG5pbXBvcnQgeyBVbml2ZXJzYWxpemVDb21tYW5kIH0gZnJvbSAnLi9jb21tYW5kcy91bml2ZXJzYWxpemUuanMnXG5pbXBvcnQgeyBWZXJzaW9uQ29tbWFuZCB9IGZyb20gJy4vY29tbWFuZHMvdmVyc2lvbi5qcydcbmltcG9ydCB7IENMSV9WRVJTSU9OIH0gZnJvbSAnLi91dGlscy9taXNjLmpzJ1xuXG5leHBvcnQgY29uc3QgY2xpID0gbmV3IENsaSh7XG4gIGJpbmFyeU5hbWU6ICduYXBpJyxcbiAgYmluYXJ5VmVyc2lvbjogQ0xJX1ZFUlNJT04sXG59KVxuXG5jbGkucmVnaXN0ZXIoTmV3Q29tbWFuZClcbmNsaS5yZWdpc3RlcihCdWlsZENvbW1hbmQpXG5jbGkucmVnaXN0ZXIoQ3JlYXRlTnBtRGlyc0NvbW1hbmQpXG5jbGkucmVnaXN0ZXIoQXJ0aWZhY3RzQ29tbWFuZClcbmNsaS5yZWdpc3RlcihVbml2ZXJzYWxpemVDb21tYW5kKVxuY2xpLnJlZ2lzdGVyKFJlbmFtZUNvbW1hbmQpXG5jbGkucmVnaXN0ZXIoUHJlUHVibGlzaENvbW1hbmQpXG5jbGkucmVnaXN0ZXIoVmVyc2lvbkNvbW1hbmQpXG5jbGkucmVnaXN0ZXIoSGVscENvbW1hbmQpXG5cbi8qKlxuICpcbiAqIEB1c2FnZVxuICpcbiAqIGBgYHRzXG4gKiBjb25zdCBjbGkgPSBuZXcgTmFwaUNsaSgpXG4gKlxuICogY2xpLmJ1aWxkKHtcbiAqICAgY3dkOiAnL3BhdGgvdG8veW91ci9wcm9qZWN0JyxcbiAqIH0pXG4gKiBgYGBcbiAqL1xuZXhwb3J0IGNsYXNzIE5hcGlDbGkge1xuICBhcnRpZmFjdHMgPSBjb2xsZWN0QXJ0aWZhY3RzXG4gIG5ldyA9IG5ld1Byb2plY3RcbiAgYnVpbGQgPSBidWlsZFByb2plY3RcbiAgY3JlYXRlTnBtRGlycyA9IGNyZWF0ZU5wbURpcnNcbiAgcHJlUHVibGlzaCA9IHByZVB1Ymxpc2hcbiAgcmVuYW1lID0gcmVuYW1lUHJvamVjdFxuICB1bml2ZXJzYWxpemUgPSB1bml2ZXJzYWxpemVCaW5hcmllc1xuICB2ZXJzaW9uID0gdmVyc2lvblxufVxuXG5leHBvcnQgZnVuY3Rpb24gY3JlYXRlQnVpbGRDb21tYW5kKGFyZ3M6IHN0cmluZ1tdKTogQnVpbGRDb21tYW5kIHtcbiAgcmV0dXJuIGNsaS5wcm9jZXNzKFsnYnVpbGQnLCAuLi5hcmdzXSkgYXMgQnVpbGRDb21tYW5kXG59XG5cbmV4cG9ydCBmdW5jdGlvbiBjcmVhdGVBcnRpZmFjdHNDb21tYW5kKGFyZ3M6IHN0cmluZ1tdKTogQXJ0aWZhY3RzQ29tbWFuZCB7XG4gIHJldHVybiBjbGkucHJvY2VzcyhbJ2FydGlmYWN0cycsIC4uLmFyZ3NdKSBhcyBBcnRpZmFjdHNDb21tYW5kXG59XG5cbmV4cG9ydCBmdW5jdGlvbiBjcmVhdGVDcmVhdGVOcG1EaXJzQ29tbWFuZChcbiAgYXJnczogc3RyaW5nW10sXG4pOiBDcmVhdGVOcG1EaXJzQ29tbWFuZCB7XG4gIHJldHVybiBjbGkucHJvY2VzcyhbJ2NyZWF0ZS1ucG0tZGlycycsIC4uLmFyZ3NdKSBhcyBDcmVhdGVOcG1EaXJzQ29tbWFuZFxufVxuXG5leHBvcnQgZnVuY3Rpb24gY3JlYXRlUHJlUHVibGlzaENvbW1hbmQoYXJnczogc3RyaW5nW10pOiBQcmVQdWJsaXNoQ29tbWFuZCB7XG4gIHJldHVybiBjbGkucHJvY2VzcyhbJ3ByZS1wdWJsaXNoJywgLi4uYXJnc10pIGFzIFByZVB1Ymxpc2hDb21tYW5kXG59XG5cbmV4cG9ydCBmdW5jdGlvbiBjcmVhdGVSZW5hbWVDb21tYW5kKGFyZ3M6IHN0cmluZ1tdKTogUmVuYW1lQ29tbWFuZCB7XG4gIHJldHVybiBjbGkucHJvY2VzcyhbJ3JlbmFtZScsIC4uLmFyZ3NdKSBhcyBSZW5hbWVDb21tYW5kXG59XG5cbmV4cG9ydCBmdW5jdGlvbiBjcmVhdGVVbml2ZXJzYWxpemVDb21tYW5kKGFyZ3M6IHN0cmluZ1tdKTogVW5pdmVyc2FsaXplQ29tbWFuZCB7XG4gIHJldHVybiBjbGkucHJvY2VzcyhbJ3VuaXZlcnNhbGl6ZScsIC4uLmFyZ3NdKSBhcyBVbml2ZXJzYWxpemVDb21tYW5kXG59XG5cbmV4cG9ydCBmdW5jdGlvbiBjcmVhdGVWZXJzaW9uQ29tbWFuZChhcmdzOiBzdHJpbmdbXSk6IFZlcnNpb25Db21tYW5kIHtcbiAgcmV0dXJuIGNsaS5wcm9jZXNzKFsndmVyc2lvbicsIC4uLmFyZ3NdKSBhcyBWZXJzaW9uQ29tbWFuZFxufVxuXG5leHBvcnQgZnVuY3Rpb24gY3JlYXRlTmV3Q29tbWFuZChhcmdzOiBzdHJpbmdbXSk6IE5ld0NvbW1hbmQge1xuICByZXR1cm4gY2xpLnByb2Nlc3MoWyduZXcnLCAuLi5hcmdzXSkgYXMgTmV3Q29tbWFuZFxufVxuXG5leHBvcnQgeyBwYXJzZVRyaXBsZSB9IGZyb20gJy4vdXRpbHMvdGFyZ2V0LmpzJ1xuZXhwb3J0IHtcbiAgdHlwZSBHZW5lcmF0ZVR5cGVEZWZPcHRpb25zLFxuICB0eXBlIFdyaXRlSnNCaW5kaW5nT3B0aW9ucyxcbiAgd3JpdGVKc0JpbmRpbmcsXG4gIGdlbmVyYXRlVHlwZURlZixcbn0gZnJvbSAnLi9hcGkvYnVpbGQuanMnXG5leHBvcnQgeyByZWFkTmFwaUNvbmZpZyB9IGZyb20gJy4vdXRpbHMvY29uZmlnLmpzJ1xuIl0sInhfZ29vZ2xlX2lnbm9yZUxpc3QiOlsxOSwyMCwyMSwyMiwyMywyNCwyNSwyNl0sIm1hcHBpbmdzIjoiOzs7Ozs7Ozs7Ozs7Ozs7Ozs7O0FBSUEsSUFBc0IsdUJBQXRCLGNBQW1ELFFBQVE7Q0FDekQsT0FBTyxRQUFRLENBQUMsQ0FBQyxZQUFZLENBQUM7Q0FFOUIsT0FBTyxRQUFRLFFBQVEsTUFBTSxFQUMzQixhQUNFLDZFQUNILENBQUM7Q0FFRixNQUFNLE9BQU8sT0FBTyxTQUFTLFFBQVEsS0FBSyxFQUFFLEVBQzFDLGFBQ0Usc0hBQ0gsQ0FBQztDQUVGLGFBQXNCLE9BQU8sT0FBTyxvQkFBb0IsRUFDdEQsYUFBYSxtQ0FDZCxDQUFDO0NBRUYsa0JBQWtCLE9BQU8sT0FBTyx1QkFBdUIsZ0JBQWdCLEVBQ3JFLGFBQWEsMEJBQ2QsQ0FBQztDQUVGLFlBQVksT0FBTyxPQUFPLHNCQUFzQixlQUFlLEVBQzdELGFBQ0UsaUdBQ0gsQ0FBQztDQUVGLFNBQVMsT0FBTyxPQUFPLGFBQWEsT0FBTyxFQUN6QyxhQUFhLGlEQUNkLENBQUM7Q0FFRixpQkFBMEIsT0FBTyxPQUFPLHNCQUFzQixFQUM1RCxhQUNFLG1GQUNILENBQUM7Q0FFRixhQUFhO0FBQ1gsU0FBTztHQUNMLEtBQUssS0FBSztHQUNWLFlBQVksS0FBSztHQUNqQixpQkFBaUIsS0FBSztHQUN0QixXQUFXLEtBQUs7R0FDaEIsUUFBUSxLQUFLO0dBQ2IsZ0JBQWdCLEtBQUs7R0FDdEI7OztB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