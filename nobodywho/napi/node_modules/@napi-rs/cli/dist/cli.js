#!/usr/bin/env node
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

//#endregion
//#region src/cli.ts
cli.runExit(process.argv.slice(2));

//#endregion
export {  };
//# sourceMappingURL=data:application/json;charset=utf-8;base64,eyJ2ZXJzaW9uIjozLCJmaWxlIjoiY2xpLmpzIiwibmFtZXMiOlsiZGVidWciLCJwYXRoIiwicGljayIsInBrZ0pzb24udmVyc2lvbiIsIlRBUkdFVF9MSU5LRVI6IFJlY29yZDxzdHJpbmcsIHN0cmluZz4iLCJDcHVUb05vZGVBcmNoOiBSZWNvcmQ8c3RyaW5nLCBOb2RlSlNBcmNoPiIsIlN5c1RvTm9kZVBsYXRmb3JtOiBSZWNvcmQ8c3RyaW5nLCBQbGF0Zm9ybT4iLCJVbmlBcmNoc0J5UGxhdGZvcm06IFBhcnRpYWw8UmVjb3JkPFBsYXRmb3JtLCBOb2RlSlNBcmNoW10+PiIsImNwdTogc3RyaW5nIiwic3lzOiBzdHJpbmciLCJhYmk6IHN0cmluZyB8IG51bGwiLCJyZXF1aXJlbWVudHM6IHN0cmluZ1tdIiwicGF0aCIsInNlcGFyYXRlZENvbmZpZzogVXNlck5hcGlDb25maWcgfCB1bmRlZmluZWQiLCJuYXBpQ29uZmlnOiBOYXBpQ29uZmlnIiwidGFyZ2V0czogc3RyaW5nW10iLCJleHBvcnRzOiBzdHJpbmdbXSIsImRlYnVnIiwiZGlyIiwiZnMiLCJmcyIsImRlYnVnIiwib3B0aW9uczogUGFyc2VkQnVpbGRPcHRpb25zIiwibWV0YWRhdGE6IENhcmdvV29ya3NwYWNlTWV0YWRhdGEiLCJjcmF0ZTogQ3JhdGUiLCJjb25maWc6IE5hcGlDb25maWciLCJhbGlhczogUmVjb3JkPHN0cmluZywgc3RyaW5nPiIsInZlcnNpb24iLCJkZXN0IiwiZGlyIiwidmFsdWUiLCJleHBvcnRzOiBzdHJpbmdbXSIsImRlYnVnIiwibWtkaXJBc3luYyIsImRpciIsInJhd01rZGlyQXN5bmMiLCJ3cml0ZUZpbGVBc3luYyIsInJhd1dyaXRlRmlsZUFzeW5jIiwic2NvcGVkUGFja2FnZUpzb246IENvbW1vblBhY2thZ2VKc29uRmllbGRzIiwicGljayIsInBhcnNlIiwiI3ByaW50T2JqZWN0IiwiI2Zvcm1hdCIsIiNpc1NpbXBseVNlcmlhbGl6YWJsZSIsInZhbHVlIiwiI2RhdGVEZWNsYXJhdGlvbiIsIiNzdHJEZWNsYXJhdGlvbiIsIiNudW1iZXJEZWNsYXJhdGlvbiIsIiNib29sRGVjbGFyYXRpb24iLCIjZ2V0VHlwZU9mQXJyYXkiLCIjYXJyYXlEZWNsYXJhdGlvbiIsIiNoZWFkZXJHcm91cCIsIiNwcmludEFzSW5saW5lVmFsdWUiLCIjZGVjbGFyYXRpb24iLCIjaGVhZGVyIiwiI2FycmF5VHlwZUNhY2hlIiwiI2RvR2V0VHlwZU9mQXJyYXkiLCIjaXNQcmltaXRpdmUiLCIjcHJpbnREYXRlIiwidmFsdWUiLCIjc291cmNlIiwiI3Bvc2l0aW9uIiwiI3doaXRlc3BhY2UiLCJ2YWx1ZSIsInRhYmxlIiwicGFyc2UiLCJqb2luIiwibWVyZ2UiLCJmbG9hdCIsInBhaXIiLCJwYXJzZSIsImlucHV0IiwiZGlyIiwid2Fsay51cCIsInBhcnNlVG9tbCIsInN0cmluZ2lmeVRvbWwiLCJmaW5kLmRpciIsInlhbWxQYXJzZSIsInlhbWxTdHJpbmdpZnkiLCJkZWJ1ZyIsImZzIiwieWFtbExvYWQiLCJqb2JzVG9SZW1vdmU6IHN0cmluZ1tdIiwieWFtbER1bXAiLCJzdGF0IiwicGF0aCIsImRlYnVnIiwiZGVidWciLCJwYWNrYWdlTmFtZSIsInZlcnNpb24iLCJyZXBvIiwib3duZXIiLCJvY3Rva2l0IiwicGtnSW5mbyIsInBrZ0luZm86IFBhY2thZ2VJbmZvIHwgdW5kZWZpbmVkIiwiZGVidWciLCJ1bml2ZXJzYWxpemVyczogUGFydGlhbDxcbiAgUmVjb3JkPE5vZGVKUy5QbGF0Zm9ybSwgKGlucHV0czogc3RyaW5nW10sIG91dHB1dDogc3RyaW5nKSA9PiB2b2lkPlxuPiIsImRlYnVnIiwidGFyZ2V0UGF0aDogc3RyaW5nIiwicGF0aCJdLCJzb3VyY2VzIjpbIi4uL3NyYy9kZWYvYXJ0aWZhY3RzLnRzIiwiLi4vc3JjL3V0aWxzL2xvZy50cyIsIi4uL3BhY2thZ2UuanNvbiIsIi4uL3NyYy91dGlscy9taXNjLnRzIiwiLi4vc3JjL3V0aWxzL3RhcmdldC50cyIsIi4uL3NyYy91dGlscy92ZXJzaW9uLnRzIiwiLi4vc3JjL3V0aWxzL21ldGFkYXRhLnRzIiwiLi4vc3JjL3V0aWxzL2NvbmZpZy50cyIsIi4uL3NyYy91dGlscy9jYXJnby50cyIsIi4uL3NyYy91dGlscy90eXBlZ2VuLnRzIiwiLi4vc3JjL3V0aWxzL3JlYWQtY29uZmlnLnRzIiwiLi4vc3JjL2FwaS9hcnRpZmFjdHMudHMiLCIuLi9zcmMvYXBpL3RlbXBsYXRlcy9qcy1iaW5kaW5nLnRzIiwiLi4vc3JjL2FwaS90ZW1wbGF0ZXMvbG9hZC13YXNpLXRlbXBsYXRlLnRzIiwiLi4vc3JjL2FwaS90ZW1wbGF0ZXMvd2FzaS13b3JrZXItdGVtcGxhdGUudHMiLCIuLi9zcmMvYXBpL2J1aWxkLnRzIiwiLi4vc3JjL2RlZi9jcmVhdGUtbnBtLWRpcnMudHMiLCIuLi9zcmMvYXBpL2NyZWF0ZS1ucG0tZGlycy50cyIsIi4uL3NyYy9kZWYvbmV3LnRzIiwiLi4vLi4vbm9kZV9tb2R1bGVzL0BzdGQvdG9tbC9zdHJpbmdpZnkuanMiLCIuLi8uLi9ub2RlX21vZHVsZXMvQGpzci9zdGRfX2NvbGxlY3Rpb25zL191dGlscy5qcyIsIi4uLy4uL25vZGVfbW9kdWxlcy9AanNyL3N0ZF9fY29sbGVjdGlvbnMvZGVlcF9tZXJnZS5qcyIsIi4uLy4uL25vZGVfbW9kdWxlcy9Ac3RkL3RvbWwvX3BhcnNlci5qcyIsIi4uLy4uL25vZGVfbW9kdWxlcy9Ac3RkL3RvbWwvcGFyc2UuanMiLCIuLi8uLi9ub2RlX21vZHVsZXMvZW1wYXRoaWMvcmVzb2x2ZS5tanMiLCIuLi8uLi9ub2RlX21vZHVsZXMvZW1wYXRoaWMvd2Fsay5tanMiLCIuLi8uLi9ub2RlX21vZHVsZXMvZW1wYXRoaWMvZmluZC5tanMiLCIuLi9zcmMvZGVmL3JlbmFtZS50cyIsIi4uL3NyYy9hcGkvcmVuYW1lLnRzIiwiLi4vc3JjL2FwaS9uZXcudHMiLCIuLi9zcmMvZGVmL3ByZS1wdWJsaXNoLnRzIiwiLi4vc3JjL2RlZi92ZXJzaW9uLnRzIiwiLi4vc3JjL2FwaS92ZXJzaW9uLnRzIiwiLi4vc3JjL2FwaS9wcmUtcHVibGlzaC50cyIsIi4uL3NyYy9kZWYvdW5pdmVyc2FsaXplLnRzIiwiLi4vc3JjL2FwaS91bml2ZXJzYWxpemUudHMiLCIuLi9zcmMvY29tbWFuZHMvYXJ0aWZhY3RzLnRzIiwiLi4vc3JjL2RlZi9idWlsZC50cyIsIi4uL3NyYy9jb21tYW5kcy9idWlsZC50cyIsIi4uL3NyYy9jb21tYW5kcy9jcmVhdGUtbnBtLWRpcnMudHMiLCIuLi9zcmMvY29tbWFuZHMvaGVscC50cyIsIi4uL3NyYy9jb21tYW5kcy9uZXcudHMiLCIuLi9zcmMvY29tbWFuZHMvcHJlLXB1Ymxpc2gudHMiLCIuLi9zcmMvY29tbWFuZHMvcmVuYW1lLnRzIiwiLi4vc3JjL2NvbW1hbmRzL3VuaXZlcnNhbGl6ZS50cyIsIi4uL3NyYy9jb21tYW5kcy92ZXJzaW9uLnRzIiwiLi4vc3JjL2luZGV4LnRzIiwiLi4vc3JjL2NsaS50cyJdLCJzb3VyY2VzQ29udGVudCI6WyIvLyBUaGlzIGZpbGUgaXMgZ2VuZXJhdGVkIGJ5IGNvZGVnZW4vaW5kZXgudHNcbi8vIERvIG5vdCBlZGl0IHRoaXMgZmlsZSBtYW51YWxseVxuaW1wb3J0IHsgQ29tbWFuZCwgT3B0aW9uIH0gZnJvbSAnY2xpcGFuaW9uJ1xuXG5leHBvcnQgYWJzdHJhY3QgY2xhc3MgQmFzZUFydGlmYWN0c0NvbW1hbmQgZXh0ZW5kcyBDb21tYW5kIHtcbiAgc3RhdGljIHBhdGhzID0gW1snYXJ0aWZhY3RzJ11dXG5cbiAgc3RhdGljIHVzYWdlID0gQ29tbWFuZC5Vc2FnZSh7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnQ29weSBhcnRpZmFjdHMgZnJvbSBHaXRodWIgQWN0aW9ucyBpbnRvIG5wbSBwYWNrYWdlcyBhbmQgcmVhZHkgdG8gcHVibGlzaCcsXG4gIH0pXG5cbiAgY3dkID0gT3B0aW9uLlN0cmluZygnLS1jd2QnLCBwcm9jZXNzLmN3ZCgpLCB7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnVGhlIHdvcmtpbmcgZGlyZWN0b3J5IG9mIHdoZXJlIG5hcGkgY29tbWFuZCB3aWxsIGJlIGV4ZWN1dGVkIGluLCBhbGwgb3RoZXIgcGF0aHMgb3B0aW9ucyBhcmUgcmVsYXRpdmUgdG8gdGhpcyBwYXRoJyxcbiAgfSlcblxuICBjb25maWdQYXRoPzogc3RyaW5nID0gT3B0aW9uLlN0cmluZygnLS1jb25maWctcGF0aCwtYycsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1BhdGggdG8gYG5hcGlgIGNvbmZpZyBqc29uIGZpbGUnLFxuICB9KVxuXG4gIHBhY2thZ2VKc29uUGF0aCA9IE9wdGlvbi5TdHJpbmcoJy0tcGFja2FnZS1qc29uLXBhdGgnLCAncGFja2FnZS5qc29uJywge1xuICAgIGRlc2NyaXB0aW9uOiAnUGF0aCB0byBgcGFja2FnZS5qc29uYCcsXG4gIH0pXG5cbiAgb3V0cHV0RGlyID0gT3B0aW9uLlN0cmluZygnLS1vdXRwdXQtZGlyLC1vLC1kJywgJy4vYXJ0aWZhY3RzJywge1xuICAgIGRlc2NyaXB0aW9uOlxuICAgICAgJ1BhdGggdG8gdGhlIGZvbGRlciB3aGVyZSBhbGwgYnVpbHQgYC5ub2RlYCBmaWxlcyBwdXQsIHNhbWUgYXMgYC0tb3V0cHV0LWRpcmAgb2YgYnVpbGQgY29tbWFuZCcsXG4gIH0pXG5cbiAgbnBtRGlyID0gT3B0aW9uLlN0cmluZygnLS1ucG0tZGlyJywgJ25wbScsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1BhdGggdG8gdGhlIGZvbGRlciB3aGVyZSB0aGUgbnBtIHBhY2thZ2VzIHB1dCcsXG4gIH0pXG5cbiAgYnVpbGRPdXRwdXREaXI/OiBzdHJpbmcgPSBPcHRpb24uU3RyaW5nKCctLWJ1aWxkLW91dHB1dC1kaXInLCB7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnUGF0aCB0byB0aGUgYnVpbGQgb3V0cHV0IGRpciwgb25seSBuZWVkZWQgd2hlbiB0YXJnZXRzIGNvbnRhaW5zIGB3YXNtMzItd2FzaS0qYCcsXG4gIH0pXG5cbiAgZ2V0T3B0aW9ucygpIHtcbiAgICByZXR1cm4ge1xuICAgICAgY3dkOiB0aGlzLmN3ZCxcbiAgICAgIGNvbmZpZ1BhdGg6IHRoaXMuY29uZmlnUGF0aCxcbiAgICAgIHBhY2thZ2VKc29uUGF0aDogdGhpcy5wYWNrYWdlSnNvblBhdGgsXG4gICAgICBvdXRwdXREaXI6IHRoaXMub3V0cHV0RGlyLFxuICAgICAgbnBtRGlyOiB0aGlzLm5wbURpcixcbiAgICAgIGJ1aWxkT3V0cHV0RGlyOiB0aGlzLmJ1aWxkT3V0cHV0RGlyLFxuICAgIH1cbiAgfVxufVxuXG4vKipcbiAqIENvcHkgYXJ0aWZhY3RzIGZyb20gR2l0aHViIEFjdGlvbnMgaW50byBucG0gcGFja2FnZXMgYW5kIHJlYWR5IHRvIHB1Ymxpc2hcbiAqL1xuZXhwb3J0IGludGVyZmFjZSBBcnRpZmFjdHNPcHRpb25zIHtcbiAgLyoqXG4gICAqIFRoZSB3b3JraW5nIGRpcmVjdG9yeSBvZiB3aGVyZSBuYXBpIGNvbW1hbmQgd2lsbCBiZSBleGVjdXRlZCBpbiwgYWxsIG90aGVyIHBhdGhzIG9wdGlvbnMgYXJlIHJlbGF0aXZlIHRvIHRoaXMgcGF0aFxuICAgKlxuICAgKiBAZGVmYXVsdCBwcm9jZXNzLmN3ZCgpXG4gICAqL1xuICBjd2Q/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gYG5hcGlgIGNvbmZpZyBqc29uIGZpbGVcbiAgICovXG4gIGNvbmZpZ1BhdGg/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gYHBhY2thZ2UuanNvbmBcbiAgICpcbiAgICogQGRlZmF1bHQgJ3BhY2thZ2UuanNvbidcbiAgICovXG4gIHBhY2thZ2VKc29uUGF0aD86IHN0cmluZ1xuICAvKipcbiAgICogUGF0aCB0byB0aGUgZm9sZGVyIHdoZXJlIGFsbCBidWlsdCBgLm5vZGVgIGZpbGVzIHB1dCwgc2FtZSBhcyBgLS1vdXRwdXQtZGlyYCBvZiBidWlsZCBjb21tYW5kXG4gICAqXG4gICAqIEBkZWZhdWx0ICcuL2FydGlmYWN0cydcbiAgICovXG4gIG91dHB1dERpcj86IHN0cmluZ1xuICAvKipcbiAgICogUGF0aCB0byB0aGUgZm9sZGVyIHdoZXJlIHRoZSBucG0gcGFja2FnZXMgcHV0XG4gICAqXG4gICAqIEBkZWZhdWx0ICducG0nXG4gICAqL1xuICBucG1EaXI/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gdGhlIGJ1aWxkIG91dHB1dCBkaXIsIG9ubHkgbmVlZGVkIHdoZW4gdGFyZ2V0cyBjb250YWlucyBgd2FzbTMyLXdhc2ktKmBcbiAgICovXG4gIGJ1aWxkT3V0cHV0RGlyPzogc3RyaW5nXG59XG5cbmV4cG9ydCBmdW5jdGlvbiBhcHBseURlZmF1bHRBcnRpZmFjdHNPcHRpb25zKG9wdGlvbnM6IEFydGlmYWN0c09wdGlvbnMpIHtcbiAgcmV0dXJuIHtcbiAgICBjd2Q6IHByb2Nlc3MuY3dkKCksXG4gICAgcGFja2FnZUpzb25QYXRoOiAncGFja2FnZS5qc29uJyxcbiAgICBvdXRwdXREaXI6ICcuL2FydGlmYWN0cycsXG4gICAgbnBtRGlyOiAnbnBtJyxcbiAgICAuLi5vcHRpb25zLFxuICB9XG59XG4iLCJpbXBvcnQgKiBhcyBjb2xvcnMgZnJvbSAnY29sb3JldHRlJ1xuaW1wb3J0IHsgY3JlYXRlRGVidWcgfSBmcm9tICdvYnVnJ1xuXG5kZWNsYXJlIG1vZHVsZSAnb2J1Zycge1xuICBpbnRlcmZhY2UgRGVidWdnZXIge1xuICAgIGluZm86IHR5cGVvZiBjb25zb2xlLmVycm9yXG4gICAgd2FybjogdHlwZW9mIGNvbnNvbGUuZXJyb3JcbiAgICBlcnJvcjogdHlwZW9mIGNvbnNvbGUuZXJyb3JcbiAgfVxufVxuXG5leHBvcnQgY29uc3QgZGVidWdGYWN0b3J5ID0gKG5hbWVzcGFjZTogc3RyaW5nKSA9PiB7XG4gIGNvbnN0IGRlYnVnID0gY3JlYXRlRGVidWcoYG5hcGk6JHtuYW1lc3BhY2V9YCwge1xuICAgIGZvcm1hdHRlcnM6IHtcbiAgICAgIC8vIGRlYnVnKCclaScsICdUaGlzIGlzIGFuIGluZm8nKVxuICAgICAgaSh2KSB7XG4gICAgICAgIHJldHVybiBjb2xvcnMuZ3JlZW4odilcbiAgICAgIH0sXG4gICAgfSxcbiAgfSlcblxuICBkZWJ1Zy5pbmZvID0gKC4uLmFyZ3M6IGFueVtdKSA9PlxuICAgIGNvbnNvbGUuZXJyb3IoY29sb3JzLmJsYWNrKGNvbG9ycy5iZ0dyZWVuKCcgSU5GTyAnKSksIC4uLmFyZ3MpXG4gIGRlYnVnLndhcm4gPSAoLi4uYXJnczogYW55W10pID0+XG4gICAgY29uc29sZS5lcnJvcihjb2xvcnMuYmxhY2soY29sb3JzLmJnWWVsbG93KCcgV0FSTklORyAnKSksIC4uLmFyZ3MpXG4gIGRlYnVnLmVycm9yID0gKC4uLmFyZ3M6IGFueVtdKSA9PlxuICAgIGNvbnNvbGUuZXJyb3IoXG4gICAgICBjb2xvcnMud2hpdGUoY29sb3JzLmJnUmVkKCcgRVJST1IgJykpLFxuICAgICAgLi4uYXJncy5tYXAoKGFyZykgPT5cbiAgICAgICAgYXJnIGluc3RhbmNlb2YgRXJyb3IgPyAoYXJnLnN0YWNrID8/IGFyZy5tZXNzYWdlKSA6IGFyZyxcbiAgICAgICksXG4gICAgKVxuXG4gIHJldHVybiBkZWJ1Z1xufVxuZXhwb3J0IGNvbnN0IGRlYnVnID0gZGVidWdGYWN0b3J5KCd1dGlscycpXG4iLCJ7XG4gIFwibmFtZVwiOiBcIkBuYXBpLXJzL2NsaVwiLFxuICBcInZlcnNpb25cIjogXCIzLjUuMVwiLFxuICBcImRlc2NyaXB0aW9uXCI6IFwiQ2xpIHRvb2xzIGZvciBuYXBpLXJzXCIsXG4gIFwiYXV0aG9yXCI6IFwiTG9uZ1lpbmFuIDxseW53ZWtsbUBnbWFpbC5jb20+XCIsXG4gIFwiaG9tZXBhZ2VcIjogXCJodHRwczovL25hcGkucnMvXCIsXG4gIFwibGljZW5zZVwiOiBcIk1JVFwiLFxuICBcInR5cGVcIjogXCJtb2R1bGVcIixcbiAgXCJlbmdpbmVzXCI6IHtcbiAgICBcIm5vZGVcIjogXCI+PSAxNlwiXG4gIH0sXG4gIFwiYmluXCI6IHtcbiAgICBcIm5hcGlcIjogXCIuL2Rpc3QvY2xpLmpzXCIsXG4gICAgXCJuYXBpLXJhd1wiOiBcIi4vY2xpLm1qc1wiXG4gIH0sXG4gIFwibWFpblwiOiBcIi4vZGlzdC9pbmRleC5janNcIixcbiAgXCJtb2R1bGVcIjogXCIuL2Rpc3QvaW5kZXguanNcIixcbiAgXCJ0eXBlc1wiOiBcIi4vZGlzdC9pbmRleC5kLnRzXCIsXG4gIFwiZXhwb3J0c1wiOiB7XG4gICAgXCIuXCI6IHtcbiAgICAgIFwiaW1wb3J0XCI6IFwiLi9kaXN0L2luZGV4LmpzXCIsXG4gICAgICBcInJlcXVpcmVcIjogXCIuL2Rpc3QvaW5kZXguY2pzXCJcbiAgICB9LFxuICAgIFwiLi9wYWNrYWdlLmpzb25cIjogXCIuL3BhY2thZ2UuanNvblwiXG4gIH0sXG4gIFwiZmlsZXNcIjogW1xuICAgIFwiZGlzdFwiLFxuICAgIFwic3JjXCIsXG4gICAgXCIhX190ZXN0c19fXCJcbiAgXSxcbiAgXCJrZXl3b3Jkc1wiOiBbXG4gICAgXCJjbGlcIixcbiAgICBcInJ1c3RcIixcbiAgICBcIm5hcGlcIixcbiAgICBcIm4tYXBpXCIsXG4gICAgXCJub2RlLWFwaVwiLFxuICAgIFwibm9kZS1hZGRvblwiLFxuICAgIFwibmVvblwiXG4gIF0sXG4gIFwibWFpbnRhaW5lcnNcIjogW1xuICAgIHtcbiAgICAgIFwibmFtZVwiOiBcIkxvbmdZaW5hblwiLFxuICAgICAgXCJlbWFpbFwiOiBcImx5bndla2xtQGdtYWlsLmNvbVwiLFxuICAgICAgXCJob21lcGFnZVwiOiBcImh0dHBzOi8vZ2l0aHViLmNvbS9Ccm9vb29vb2tseW5cIlxuICAgIH0sXG4gICAge1xuICAgICAgXCJuYW1lXCI6IFwiZm9yZWhhbG9cIixcbiAgICAgIFwiaG9tZXBhZ2VcIjogXCJodHRwczovL2dpdGh1Yi5jb20vZm9yZWhhbG9cIlxuICAgIH1cbiAgXSxcbiAgXCJyZXBvc2l0b3J5XCI6IHtcbiAgICBcInR5cGVcIjogXCJnaXRcIixcbiAgICBcInVybFwiOiBcImdpdCtodHRwczovL2dpdGh1Yi5jb20vbmFwaS1ycy9uYXBpLXJzLmdpdFwiXG4gIH0sXG4gIFwicHVibGlzaENvbmZpZ1wiOiB7XG4gICAgXCJyZWdpc3RyeVwiOiBcImh0dHBzOi8vcmVnaXN0cnkubnBtanMub3JnL1wiLFxuICAgIFwiYWNjZXNzXCI6IFwicHVibGljXCJcbiAgfSxcbiAgXCJidWdzXCI6IHtcbiAgICBcInVybFwiOiBcImh0dHBzOi8vZ2l0aHViLmNvbS9uYXBpLXJzL25hcGktcnMvaXNzdWVzXCJcbiAgfSxcbiAgXCJkZXBlbmRlbmNpZXNcIjoge1xuICAgIFwiQGlucXVpcmVyL3Byb21wdHNcIjogXCJeOC4wLjBcIixcbiAgICBcIkBuYXBpLXJzL2Nyb3NzLXRvb2xjaGFpblwiOiBcIl4xLjAuM1wiLFxuICAgIFwiQG5hcGktcnMvd2FzbS10b29sc1wiOiBcIl4xLjAuMVwiLFxuICAgIFwiQG9jdG9raXQvcmVzdFwiOiBcIl4yMi4wLjFcIixcbiAgICBcImNsaXBhbmlvblwiOiBcIl40LjAuMC1yYy40XCIsXG4gICAgXCJjb2xvcmV0dGVcIjogXCJeMi4wLjIwXCIsXG4gICAgXCJlbW5hcGlcIjogXCJeMS43LjFcIixcbiAgICBcImVzLXRvb2xraXRcIjogXCJeMS40MS4wXCIsXG4gICAgXCJqcy15YW1sXCI6IFwiXjQuMS4wXCIsXG4gICAgXCJvYnVnXCI6IFwiXjIuMC4wXCIsXG4gICAgXCJzZW12ZXJcIjogXCJeNy43LjNcIixcbiAgICBcInR5cGFuaW9uXCI6IFwiXjMuMTQuMFwiXG4gIH0sXG4gIFwiZGV2RGVwZW5kZW5jaWVzXCI6IHtcbiAgICBcIkBlbW5hcGkvcnVudGltZVwiOiBcIl4xLjcuMVwiLFxuICAgIFwiQG94Yy1ub2RlL2NvcmVcIjogXCJeMC4wLjM1XCIsXG4gICAgXCJAc3RkL3RvbWxcIjogXCJucG06QGpzci9zdGRfX3RvbWxAXjEuMC4xMVwiLFxuICAgIFwiQHR5cGVzL2lucXVpcmVyXCI6IFwiXjkuMC45XCIsXG4gICAgXCJAdHlwZXMvanMteWFtbFwiOiBcIl40LjAuOVwiLFxuICAgIFwiQHR5cGVzL25vZGVcIjogXCJeMjQuMTAuMFwiLFxuICAgIFwiQHR5cGVzL3NlbXZlclwiOiBcIl43LjcuMVwiLFxuICAgIFwiYXZhXCI6IFwiXjYuNC4xXCIsXG4gICAgXCJlbXBhdGhpY1wiOiBcIl4yLjAuMFwiLFxuICAgIFwiZW52LXBhdGhzXCI6IFwiXjMuMC4wXCIsXG4gICAgXCJwcmV0dGllclwiOiBcIl4zLjYuMlwiLFxuICAgIFwidHNkb3duXCI6IFwiXjAuMTguMFwiLFxuICAgIFwidHNsaWJcIjogXCJeMi44LjFcIixcbiAgICBcInR5cGVzY3JpcHRcIjogXCJeNS45LjNcIlxuICB9LFxuICBcInBlZXJEZXBlbmRlbmNpZXNcIjoge1xuICAgIFwiQGVtbmFwaS9ydW50aW1lXCI6IFwiXjEuNy4xXCJcbiAgfSxcbiAgXCJwZWVyRGVwZW5kZW5jaWVzTWV0YVwiOiB7XG4gICAgXCJAZW1uYXBpL3J1bnRpbWVcIjoge1xuICAgICAgXCJvcHRpb25hbFwiOiB0cnVlXG4gICAgfVxuICB9LFxuICBcImZ1bmRpbmdcIjoge1xuICAgIFwidHlwZVwiOiBcImdpdGh1YlwiLFxuICAgIFwidXJsXCI6IFwiaHR0cHM6Ly9naXRodWIuY29tL3Nwb25zb3JzL0Jyb29vb29va2x5blwiXG4gIH0sXG4gIFwic2NyaXB0c1wiOiB7XG4gICAgXCJjb2RlZ2VuXCI6IFwibm9kZSAtLWltcG9ydCBAb3hjLW5vZGUvY29yZS9yZWdpc3RlciAuL2NvZGVnZW4vaW5kZXgudHNcIixcbiAgICBcImJ1aWxkXCI6IFwidHNkb3duXCIsXG4gICAgXCJ0ZXN0XCI6IFwibm9kZSAtLWltcG9ydCBAb3hjLW5vZGUvY29yZS9yZWdpc3RlciAuLi9ub2RlX21vZHVsZXMvYXZhL2VudHJ5cG9pbnRzL2NsaS5tanNcIlxuICB9LFxuICBcImF2YVwiOiB7XG4gICAgXCJleHRlbnNpb25zXCI6IHtcbiAgICAgIFwidHNcIjogXCJtb2R1bGVcIlxuICAgIH0sXG4gICAgXCJ0aW1lb3V0XCI6IFwiMm1cIixcbiAgICBcIndvcmtlclRocmVhZHNcIjogZmFsc2UsXG4gICAgXCJmaWxlc1wiOiBbXG4gICAgICBcIioqL19fdGVzdHNfXy8qKi8qLnNwZWMudHNcIixcbiAgICAgIFwiZTJlLyoqLyouc3BlYy50c1wiXG4gICAgXVxuICB9XG59XG4iLCJpbXBvcnQge1xuICByZWFkRmlsZSxcbiAgd3JpdGVGaWxlLFxuICB1bmxpbmssXG4gIGNvcHlGaWxlLFxuICBta2RpcixcbiAgc3RhdCxcbiAgcmVhZGRpcixcbiAgYWNjZXNzLFxufSBmcm9tICdub2RlOmZzL3Byb21pc2VzJ1xuXG5pbXBvcnQgcGtnSnNvbiBmcm9tICcuLi8uLi9wYWNrYWdlLmpzb24nIHdpdGggeyB0eXBlOiAnanNvbicgfVxuaW1wb3J0IHsgZGVidWcgfSBmcm9tICcuL2xvZy5qcydcblxuZXhwb3J0IGNvbnN0IHJlYWRGaWxlQXN5bmMgPSByZWFkRmlsZVxuZXhwb3J0IGNvbnN0IHdyaXRlRmlsZUFzeW5jID0gd3JpdGVGaWxlXG5leHBvcnQgY29uc3QgdW5saW5rQXN5bmMgPSB1bmxpbmtcbmV4cG9ydCBjb25zdCBjb3B5RmlsZUFzeW5jID0gY29weUZpbGVcbmV4cG9ydCBjb25zdCBta2RpckFzeW5jID0gbWtkaXJcbmV4cG9ydCBjb25zdCBzdGF0QXN5bmMgPSBzdGF0XG5leHBvcnQgY29uc3QgcmVhZGRpckFzeW5jID0gcmVhZGRpclxuXG5leHBvcnQgZnVuY3Rpb24gZmlsZUV4aXN0cyhwYXRoOiBzdHJpbmcpOiBQcm9taXNlPGJvb2xlYW4+IHtcbiAgcmV0dXJuIGFjY2VzcyhwYXRoKS50aGVuKFxuICAgICgpID0+IHRydWUsXG4gICAgKCkgPT4gZmFsc2UsXG4gIClcbn1cblxuZXhwb3J0IGFzeW5jIGZ1bmN0aW9uIGRpckV4aXN0c0FzeW5jKHBhdGg6IHN0cmluZykge1xuICB0cnkge1xuICAgIGNvbnN0IHN0YXRzID0gYXdhaXQgc3RhdEFzeW5jKHBhdGgpXG4gICAgcmV0dXJuIHN0YXRzLmlzRGlyZWN0b3J5KClcbiAgfSBjYXRjaCB7XG4gICAgcmV0dXJuIGZhbHNlXG4gIH1cbn1cblxuZXhwb3J0IGZ1bmN0aW9uIHBpY2s8TywgSyBleHRlbmRzIGtleW9mIE8+KG86IE8sIC4uLmtleXM6IEtbXSk6IFBpY2s8TywgSz4ge1xuICByZXR1cm4ga2V5cy5yZWR1Y2UoKGFjYywga2V5KSA9PiB7XG4gICAgYWNjW2tleV0gPSBvW2tleV1cbiAgICByZXR1cm4gYWNjXG4gIH0sIHt9IGFzIE8pXG59XG5cbmV4cG9ydCBhc3luYyBmdW5jdGlvbiB1cGRhdGVQYWNrYWdlSnNvbihcbiAgcGF0aDogc3RyaW5nLFxuICBwYXJ0aWFsOiBSZWNvcmQ8c3RyaW5nLCBhbnk+LFxuKSB7XG4gIGNvbnN0IGV4aXN0cyA9IGF3YWl0IGZpbGVFeGlzdHMocGF0aClcbiAgaWYgKCFleGlzdHMpIHtcbiAgICBkZWJ1ZyhgRmlsZSBub3QgZXhpc3RzICR7cGF0aH1gKVxuICAgIHJldHVyblxuICB9XG4gIGNvbnN0IG9sZCA9IEpTT04ucGFyc2UoYXdhaXQgcmVhZEZpbGVBc3luYyhwYXRoLCAndXRmOCcpKVxuICBhd2FpdCB3cml0ZUZpbGVBc3luYyhwYXRoLCBKU09OLnN0cmluZ2lmeSh7IC4uLm9sZCwgLi4ucGFydGlhbCB9LCBudWxsLCAyKSlcbn1cblxuZXhwb3J0IGNvbnN0IENMSV9WRVJTSU9OID0gcGtnSnNvbi52ZXJzaW9uXG4iLCJpbXBvcnQgeyBleGVjU3luYyB9IGZyb20gJ25vZGU6Y2hpbGRfcHJvY2VzcydcblxuZXhwb3J0IHR5cGUgUGxhdGZvcm0gPSBOb2RlSlMuUGxhdGZvcm0gfCAnd2FzbScgfCAnd2FzaScgfCAnb3Blbmhhcm1vbnknXG5cbmV4cG9ydCBjb25zdCBVTklWRVJTQUxfVEFSR0VUUyA9IHtcbiAgJ3VuaXZlcnNhbC1hcHBsZS1kYXJ3aW4nOiBbJ2FhcmNoNjQtYXBwbGUtZGFyd2luJywgJ3g4Nl82NC1hcHBsZS1kYXJ3aW4nXSxcbn0gYXMgY29uc3RcblxuY29uc3QgU1VCX1NZU1RFTVMgPSBuZXcgU2V0KFsnYW5kcm9pZCcsICdvaG9zJ10pXG5cbmV4cG9ydCBjb25zdCBBVkFJTEFCTEVfVEFSR0VUUyA9IFtcbiAgJ2FhcmNoNjQtYXBwbGUtZGFyd2luJyxcbiAgJ2FhcmNoNjQtbGludXgtYW5kcm9pZCcsXG4gICdhYXJjaDY0LXVua25vd24tbGludXgtZ251JyxcbiAgJ2FhcmNoNjQtdW5rbm93bi1saW51eC1tdXNsJyxcbiAgJ2FhcmNoNjQtdW5rbm93bi1saW51eC1vaG9zJyxcbiAgJ2FhcmNoNjQtcGMtd2luZG93cy1tc3ZjJyxcbiAgJ3g4Nl82NC1hcHBsZS1kYXJ3aW4nLFxuICAneDg2XzY0LXBjLXdpbmRvd3MtbXN2YycsXG4gICd4ODZfNjQtcGMtd2luZG93cy1nbnUnLFxuICAneDg2XzY0LXVua25vd24tbGludXgtZ251JyxcbiAgJ3g4Nl82NC11bmtub3duLWxpbnV4LW11c2wnLFxuICAneDg2XzY0LXVua25vd24tbGludXgtb2hvcycsXG4gICd4ODZfNjQtdW5rbm93bi1mcmVlYnNkJyxcbiAgJ2k2ODYtcGMtd2luZG93cy1tc3ZjJyxcbiAgJ2FybXY3LXVua25vd24tbGludXgtZ251ZWFiaWhmJyxcbiAgJ2FybXY3LXVua25vd24tbGludXgtbXVzbGVhYmloZicsXG4gICdhcm12Ny1saW51eC1hbmRyb2lkZWFiaScsXG4gICd1bml2ZXJzYWwtYXBwbGUtZGFyd2luJyxcbiAgJ2xvb25nYXJjaDY0LXVua25vd24tbGludXgtZ251JyxcbiAgJ3Jpc2N2NjRnYy11bmtub3duLWxpbnV4LWdudScsXG4gICdwb3dlcnBjNjRsZS11bmtub3duLWxpbnV4LWdudScsXG4gICdzMzkweC11bmtub3duLWxpbnV4LWdudScsXG4gICd3YXNtMzItd2FzaS1wcmV2aWV3MS10aHJlYWRzJyxcbiAgJ3dhc20zMi13YXNpcDEtdGhyZWFkcycsXG5dIGFzIGNvbnN0XG5cbmV4cG9ydCB0eXBlIFRhcmdldFRyaXBsZSA9ICh0eXBlb2YgQVZBSUxBQkxFX1RBUkdFVFMpW251bWJlcl1cblxuZXhwb3J0IGNvbnN0IERFRkFVTFRfVEFSR0VUUyA9IFtcbiAgJ3g4Nl82NC1hcHBsZS1kYXJ3aW4nLFxuICAnYWFyY2g2NC1hcHBsZS1kYXJ3aW4nLFxuICAneDg2XzY0LXBjLXdpbmRvd3MtbXN2YycsXG4gICd4ODZfNjQtdW5rbm93bi1saW51eC1nbnUnLFxuXSBhcyBjb25zdFxuXG5leHBvcnQgY29uc3QgVEFSR0VUX0xJTktFUjogUmVjb3JkPHN0cmluZywgc3RyaW5nPiA9IHtcbiAgJ2FhcmNoNjQtdW5rbm93bi1saW51eC1tdXNsJzogJ2FhcmNoNjQtbGludXgtbXVzbC1nY2MnLFxuICAvLyBUT0RPOiBTd2l0Y2ggdG8gbG9vbmdhcmNoNjQtbGludXgtZ251LWdjYyB3aGVuIGF2YWlsYWJsZVxuICAnbG9vbmdhcmNoNjQtdW5rbm93bi1saW51eC1nbnUnOiAnbG9vbmdhcmNoNjQtbGludXgtZ251LWdjYy0xMycsXG4gICdyaXNjdjY0Z2MtdW5rbm93bi1saW51eC1nbnUnOiAncmlzY3Y2NC1saW51eC1nbnUtZ2NjJyxcbiAgJ3Bvd2VycGM2NGxlLXVua25vd24tbGludXgtZ251JzogJ3Bvd2VycGM2NGxlLWxpbnV4LWdudS1nY2MnLFxuICAnczM5MHgtdW5rbm93bi1saW51eC1nbnUnOiAnczM5MHgtbGludXgtZ251LWdjYycsXG59XG5cbi8vIGh0dHBzOi8vbm9kZWpzLm9yZy9hcGkvcHJvY2Vzcy5odG1sI3Byb2Nlc3NfcHJvY2Vzc19hcmNoXG50eXBlIE5vZGVKU0FyY2ggPVxuICB8ICdhcm0nXG4gIHwgJ2FybTY0J1xuICB8ICdpYTMyJ1xuICB8ICdsb29uZzY0J1xuICB8ICdtaXBzJ1xuICB8ICdtaXBzZWwnXG4gIHwgJ3BwYydcbiAgfCAncHBjNjQnXG4gIHwgJ3Jpc2N2NjQnXG4gIHwgJ3MzOTAnXG4gIHwgJ3MzOTB4J1xuICB8ICd4MzInXG4gIHwgJ3g2NCdcbiAgfCAndW5pdmVyc2FsJ1xuICB8ICd3YXNtMzInXG5cbmNvbnN0IENwdVRvTm9kZUFyY2g6IFJlY29yZDxzdHJpbmcsIE5vZGVKU0FyY2g+ID0ge1xuICB4ODZfNjQ6ICd4NjQnLFxuICBhYXJjaDY0OiAnYXJtNjQnLFxuICBpNjg2OiAnaWEzMicsXG4gIGFybXY3OiAnYXJtJyxcbiAgbG9vbmdhcmNoNjQ6ICdsb29uZzY0JyxcbiAgcmlzY3Y2NGdjOiAncmlzY3Y2NCcsXG4gIHBvd2VycGM2NGxlOiAncHBjNjQnLFxufVxuXG5leHBvcnQgY29uc3QgTm9kZUFyY2hUb0NwdTogUmVjb3JkPHN0cmluZywgc3RyaW5nPiA9IHtcbiAgeDY0OiAneDg2XzY0JyxcbiAgYXJtNjQ6ICdhYXJjaDY0JyxcbiAgaWEzMjogJ2k2ODYnLFxuICBhcm06ICdhcm12NycsXG4gIGxvb25nNjQ6ICdsb29uZ2FyY2g2NCcsXG4gIHJpc2N2NjQ6ICdyaXNjdjY0Z2MnLFxuICBwcGM2NDogJ3Bvd2VycGM2NGxlJyxcbn1cblxuY29uc3QgU3lzVG9Ob2RlUGxhdGZvcm06IFJlY29yZDxzdHJpbmcsIFBsYXRmb3JtPiA9IHtcbiAgbGludXg6ICdsaW51eCcsXG4gIGZyZWVic2Q6ICdmcmVlYnNkJyxcbiAgZGFyd2luOiAnZGFyd2luJyxcbiAgd2luZG93czogJ3dpbjMyJyxcbiAgb2hvczogJ29wZW5oYXJtb255Jyxcbn1cblxuZXhwb3J0IGNvbnN0IFVuaUFyY2hzQnlQbGF0Zm9ybTogUGFydGlhbDxSZWNvcmQ8UGxhdGZvcm0sIE5vZGVKU0FyY2hbXT4+ID0ge1xuICBkYXJ3aW46IFsneDY0JywgJ2FybTY0J10sXG59XG5cbmV4cG9ydCBpbnRlcmZhY2UgVGFyZ2V0IHtcbiAgdHJpcGxlOiBzdHJpbmdcbiAgcGxhdGZvcm1BcmNoQUJJOiBzdHJpbmdcbiAgcGxhdGZvcm06IFBsYXRmb3JtXG4gIGFyY2g6IE5vZGVKU0FyY2hcbiAgYWJpOiBzdHJpbmcgfCBudWxsXG59XG5cbi8qKlxuICogQSB0cmlwbGUgaXMgYSBzcGVjaWZpYyBmb3JtYXQgZm9yIHNwZWNpZnlpbmcgYSB0YXJnZXQgYXJjaGl0ZWN0dXJlLlxuICogVHJpcGxlcyBtYXkgYmUgcmVmZXJyZWQgdG8gYXMgYSB0YXJnZXQgdHJpcGxlIHdoaWNoIGlzIHRoZSBhcmNoaXRlY3R1cmUgZm9yIHRoZSBhcnRpZmFjdCBwcm9kdWNlZCwgYW5kIHRoZSBob3N0IHRyaXBsZSB3aGljaCBpcyB0aGUgYXJjaGl0ZWN0dXJlIHRoYXQgdGhlIGNvbXBpbGVyIGlzIHJ1bm5pbmcgb24uXG4gKiBUaGUgZ2VuZXJhbCBmb3JtYXQgb2YgdGhlIHRyaXBsZSBpcyBgPGFyY2g+PHN1Yj4tPHZlbmRvcj4tPHN5cz4tPGFiaT5gIHdoZXJlOlxuICogICAtIGBhcmNoYCA9IFRoZSBiYXNlIENQVSBhcmNoaXRlY3R1cmUsIGZvciBleGFtcGxlIGB4ODZfNjRgLCBgaTY4NmAsIGBhcm1gLCBgdGh1bWJgLCBgbWlwc2AsIGV0Yy5cbiAqICAgLSBgc3ViYCA9IFRoZSBDUFUgc3ViLWFyY2hpdGVjdHVyZSwgZm9yIGV4YW1wbGUgYGFybWAgaGFzIGB2N2AsIGB2N3NgLCBgdjV0ZWAsIGV0Yy5cbiAqICAgLSBgdmVuZG9yYCA9IFRoZSB2ZW5kb3IsIGZvciBleGFtcGxlIGB1bmtub3duYCwgYGFwcGxlYCwgYHBjYCwgYG52aWRpYWAsIGV0Yy5cbiAqICAgLSBgc3lzYCA9IFRoZSBzeXN0ZW0gbmFtZSwgZm9yIGV4YW1wbGUgYGxpbnV4YCwgYHdpbmRvd3NgLCBgZGFyd2luYCwgZXRjLiBub25lIGlzIHR5cGljYWxseSB1c2VkIGZvciBiYXJlLW1ldGFsIHdpdGhvdXQgYW4gT1MuXG4gKiAgIC0gYGFiaWAgPSBUaGUgQUJJLCBmb3IgZXhhbXBsZSBgZ251YCwgYGFuZHJvaWRgLCBgZWFiaWAsIGV0Yy5cbiAqL1xuZXhwb3J0IGZ1bmN0aW9uIHBhcnNlVHJpcGxlKHJhd1RyaXBsZTogc3RyaW5nKTogVGFyZ2V0IHtcbiAgaWYgKFxuICAgIHJhd1RyaXBsZSA9PT0gJ3dhc20zMi13YXNpJyB8fFxuICAgIHJhd1RyaXBsZSA9PT0gJ3dhc20zMi13YXNpLXByZXZpZXcxLXRocmVhZHMnIHx8XG4gICAgcmF3VHJpcGxlLnN0YXJ0c1dpdGgoJ3dhc20zMi13YXNpcCcpXG4gICkge1xuICAgIHJldHVybiB7XG4gICAgICB0cmlwbGU6IHJhd1RyaXBsZSxcbiAgICAgIHBsYXRmb3JtQXJjaEFCSTogJ3dhc20zMi13YXNpJyxcbiAgICAgIHBsYXRmb3JtOiAnd2FzaScsXG4gICAgICBhcmNoOiAnd2FzbTMyJyxcbiAgICAgIGFiaTogJ3dhc2knLFxuICAgIH1cbiAgfVxuICBjb25zdCB0cmlwbGUgPSByYXdUcmlwbGUuZW5kc1dpdGgoJ2VhYmknKVxuICAgID8gYCR7cmF3VHJpcGxlLnNsaWNlKDAsIC00KX0tZWFiaWBcbiAgICA6IHJhd1RyaXBsZVxuICBjb25zdCB0cmlwbGVzID0gdHJpcGxlLnNwbGl0KCctJylcbiAgbGV0IGNwdTogc3RyaW5nXG4gIGxldCBzeXM6IHN0cmluZ1xuICBsZXQgYWJpOiBzdHJpbmcgfCBudWxsID0gbnVsbFxuICBpZiAodHJpcGxlcy5sZW5ndGggPT09IDIpIHtcbiAgICAvLyBhYXJjaDY0LWZ1Y2hzaWFcbiAgICAvLyBeIGNwdSAgIF4gc3lzXG4gICAgO1tjcHUsIHN5c10gPSB0cmlwbGVzXG4gIH0gZWxzZSB7XG4gICAgLy8gYWFyY2g2NC11bmtub3duLWxpbnV4LW11c2xcbiAgICAvLyBeIGNwdSAgIF52ZW5kb3IgXiBzeXMgXiBhYmlcbiAgICAvLyBhYXJjaDY0LWFwcGxlLWRhcndpblxuICAgIC8vIF4gY3B1ICAgICAgICAgXiBzeXMgIChhYmkgaXMgTm9uZSlcbiAgICA7W2NwdSwgLCBzeXMsIGFiaSA9IG51bGxdID0gdHJpcGxlc1xuICB9XG5cbiAgaWYgKGFiaSAmJiBTVUJfU1lTVEVNUy5oYXMoYWJpKSkge1xuICAgIHN5cyA9IGFiaVxuICAgIGFiaSA9IG51bGxcbiAgfVxuICBjb25zdCBwbGF0Zm9ybSA9IFN5c1RvTm9kZVBsYXRmb3JtW3N5c10gPz8gKHN5cyBhcyBQbGF0Zm9ybSlcbiAgY29uc3QgYXJjaCA9IENwdVRvTm9kZUFyY2hbY3B1XSA/PyAoY3B1IGFzIE5vZGVKU0FyY2gpXG5cbiAgcmV0dXJuIHtcbiAgICB0cmlwbGU6IHJhd1RyaXBsZSxcbiAgICBwbGF0Zm9ybUFyY2hBQkk6IGFiaSA/IGAke3BsYXRmb3JtfS0ke2FyY2h9LSR7YWJpfWAgOiBgJHtwbGF0Zm9ybX0tJHthcmNofWAsXG4gICAgcGxhdGZvcm0sXG4gICAgYXJjaCxcbiAgICBhYmksXG4gIH1cbn1cblxuZXhwb3J0IGZ1bmN0aW9uIGdldFN5c3RlbURlZmF1bHRUYXJnZXQoKTogVGFyZ2V0IHtcbiAgY29uc3QgaG9zdCA9IGV4ZWNTeW5jKGBydXN0YyAtdlZgLCB7XG4gICAgZW52OiBwcm9jZXNzLmVudixcbiAgfSlcbiAgICAudG9TdHJpbmcoJ3V0ZjgnKVxuICAgIC5zcGxpdCgnXFxuJylcbiAgICAuZmluZCgobGluZSkgPT4gbGluZS5zdGFydHNXaXRoKCdob3N0OiAnKSlcbiAgY29uc3QgdHJpcGxlID0gaG9zdD8uc2xpY2UoJ2hvc3Q6ICcubGVuZ3RoKVxuICBpZiAoIXRyaXBsZSkge1xuICAgIHRocm93IG5ldyBUeXBlRXJyb3IoYENhbiBub3QgcGFyc2UgdGFyZ2V0IHRyaXBsZSBmcm9tIGhvc3RgKVxuICB9XG4gIHJldHVybiBwYXJzZVRyaXBsZSh0cmlwbGUpXG59XG5cbmV4cG9ydCBmdW5jdGlvbiBnZXRUYXJnZXRMaW5rZXIodGFyZ2V0OiBzdHJpbmcpOiBzdHJpbmcgfCB1bmRlZmluZWQge1xuICByZXR1cm4gVEFSR0VUX0xJTktFUlt0YXJnZXRdXG59XG5cbmV4cG9ydCBmdW5jdGlvbiB0YXJnZXRUb0VudlZhcih0YXJnZXQ6IHN0cmluZyk6IHN0cmluZyB7XG4gIHJldHVybiB0YXJnZXQucmVwbGFjZSgvLS9nLCAnXycpLnRvVXBwZXJDYXNlKClcbn1cbiIsImV4cG9ydCBlbnVtIE5hcGlWZXJzaW9uIHtcbiAgTmFwaTEgPSAxLFxuICBOYXBpMixcbiAgTmFwaTMsXG4gIE5hcGk0LFxuICBOYXBpNSxcbiAgTmFwaTYsXG4gIE5hcGk3LFxuICBOYXBpOCxcbiAgTmFwaTksXG59XG5cbi8vLyBiZWNhdXNlIG5vZGUgc3VwcG9ydCBuZXcgbmFwaSB2ZXJzaW9uIGluIHNvbWUgbWlub3IgdmVyc2lvbiB1cGRhdGVzLCBzbyB3ZSBtaWdodCBtZWV0IHN1Y2ggc2l0dWF0aW9uOlxuLy8vIGBub2RlIHYxMC4yMC4wYCBzdXBwb3J0cyBgbmFwaTVgIGFuZCBgbmFwaTZgLCBidXQgYG5vZGUgdjEyLjAuMGAgb25seSBzdXBwb3J0IGBuYXBpNGAsXG4vLy8gYnkgd2hpY2gsIHdlIGNhbiBub3QgdGVsbCBkaXJlY3RseSBuYXBpIHZlcnNpb24gc3VwcG9ydGxlc3MgZnJvbSBub2RlIHZlcnNpb24gZGlyZWN0bHkuXG5jb25zdCBOQVBJX1ZFUlNJT05fTUFUUklYID0gbmV3IE1hcDxOYXBpVmVyc2lvbiwgc3RyaW5nPihbXG4gIFtOYXBpVmVyc2lvbi5OYXBpMSwgJzguNi4wIHwgOS4wLjAgfCAxMC4wLjAnXSxcbiAgW05hcGlWZXJzaW9uLk5hcGkyLCAnOC4xMC4wIHwgOS4zLjAgfCAxMC4wLjAnXSxcbiAgW05hcGlWZXJzaW9uLk5hcGkzLCAnNi4xNC4yIHwgOC4xMS4yIHwgOS4xMS4wIHwgMTAuMC4wJ10sXG4gIFtOYXBpVmVyc2lvbi5OYXBpNCwgJzEwLjE2LjAgfCAxMS44LjAgfCAxMi4wLjAnXSxcbiAgW05hcGlWZXJzaW9uLk5hcGk1LCAnMTAuMTcuMCB8IDEyLjExLjAgfCAxMy4wLjAnXSxcbiAgW05hcGlWZXJzaW9uLk5hcGk2LCAnMTAuMjAuMCB8IDEyLjE3LjAgfCAxNC4wLjAnXSxcbiAgW05hcGlWZXJzaW9uLk5hcGk3LCAnMTAuMjMuMCB8IDEyLjE5LjAgfCAxNC4xMi4wIHwgMTUuMC4wJ10sXG4gIFtOYXBpVmVyc2lvbi5OYXBpOCwgJzEyLjIyLjAgfCAxNC4xNy4wIHwgMTUuMTIuMCB8IDE2LjAuMCddLFxuICBbTmFwaVZlcnNpb24uTmFwaTksICcxOC4xNy4wIHwgMjAuMy4wIHwgMjEuMS4wJ10sXG5dKVxuXG5pbnRlcmZhY2UgTm9kZVZlcnNpb24ge1xuICBtYWpvcjogbnVtYmVyXG4gIG1pbm9yOiBudW1iZXJcbiAgcGF0Y2g6IG51bWJlclxufVxuXG5mdW5jdGlvbiBwYXJzZU5vZGVWZXJzaW9uKHY6IHN0cmluZyk6IE5vZGVWZXJzaW9uIHtcbiAgY29uc3QgbWF0Y2hlcyA9IHYubWF0Y2goL3Y/KFswLTldKylcXC4oWzAtOV0rKVxcLihbMC05XSspL2kpXG5cbiAgaWYgKCFtYXRjaGVzKSB7XG4gICAgdGhyb3cgbmV3IEVycm9yKCdVbmtub3duIG5vZGUgdmVyc2lvbiBudW1iZXI6ICcgKyB2KVxuICB9XG5cbiAgY29uc3QgWywgbWFqb3IsIG1pbm9yLCBwYXRjaF0gPSBtYXRjaGVzXG5cbiAgcmV0dXJuIHtcbiAgICBtYWpvcjogcGFyc2VJbnQobWFqb3IpLFxuICAgIG1pbm9yOiBwYXJzZUludChtaW5vciksXG4gICAgcGF0Y2g6IHBhcnNlSW50KHBhdGNoKSxcbiAgfVxufVxuXG5mdW5jdGlvbiByZXF1aXJlZE5vZGVWZXJzaW9ucyhuYXBpVmVyc2lvbjogTmFwaVZlcnNpb24pOiBOb2RlVmVyc2lvbltdIHtcbiAgY29uc3QgcmVxdWlyZW1lbnQgPSBOQVBJX1ZFUlNJT05fTUFUUklYLmdldChuYXBpVmVyc2lvbilcblxuICBpZiAoIXJlcXVpcmVtZW50KSB7XG4gICAgcmV0dXJuIFtwYXJzZU5vZGVWZXJzaW9uKCcxMC4wLjAnKV1cbiAgfVxuXG4gIHJldHVybiByZXF1aXJlbWVudC5zcGxpdCgnfCcpLm1hcChwYXJzZU5vZGVWZXJzaW9uKVxufVxuXG5mdW5jdGlvbiB0b0VuZ2luZVJlcXVpcmVtZW50KHZlcnNpb25zOiBOb2RlVmVyc2lvbltdKTogc3RyaW5nIHtcbiAgY29uc3QgcmVxdWlyZW1lbnRzOiBzdHJpbmdbXSA9IFtdXG4gIHZlcnNpb25zLmZvckVhY2goKHYsIGkpID0+IHtcbiAgICBsZXQgcmVxID0gJydcbiAgICBpZiAoaSAhPT0gMCkge1xuICAgICAgY29uc3QgbGFzdFZlcnNpb24gPSB2ZXJzaW9uc1tpIC0gMV1cbiAgICAgIHJlcSArPSBgPCAke2xhc3RWZXJzaW9uLm1ham9yICsgMX1gXG4gICAgfVxuXG4gICAgcmVxICs9IGAke2kgPT09IDAgPyAnJyA6ICcgfHwgJ30+PSAke3YubWFqb3J9LiR7di5taW5vcn0uJHt2LnBhdGNofWBcbiAgICByZXF1aXJlbWVudHMucHVzaChyZXEpXG4gIH0pXG5cbiAgcmV0dXJuIHJlcXVpcmVtZW50cy5qb2luKCcgJylcbn1cblxuZXhwb3J0IGZ1bmN0aW9uIG5hcGlFbmdpbmVSZXF1aXJlbWVudChuYXBpVmVyc2lvbjogTmFwaVZlcnNpb24pOiBzdHJpbmcge1xuICByZXR1cm4gdG9FbmdpbmVSZXF1aXJlbWVudChyZXF1aXJlZE5vZGVWZXJzaW9ucyhuYXBpVmVyc2lvbikpXG59XG4iLCJpbXBvcnQgeyBzcGF3biB9IGZyb20gJ25vZGU6Y2hpbGRfcHJvY2VzcydcbmltcG9ydCBmcyBmcm9tICdub2RlOmZzJ1xuXG5leHBvcnQgdHlwZSBDcmF0ZVRhcmdldEtpbmQgPVxuICB8ICdiaW4nXG4gIHwgJ2V4YW1wbGUnXG4gIHwgJ3Rlc3QnXG4gIHwgJ2JlbmNoJ1xuICB8ICdsaWInXG4gIHwgJ3JsaWInXG4gIHwgJ2NkeWxpYidcbiAgfCAnY3VzdG9tLWJ1aWxkJ1xuXG5leHBvcnQgaW50ZXJmYWNlIENyYXRlVGFyZ2V0IHtcbiAgbmFtZTogc3RyaW5nXG4gIGtpbmQ6IENyYXRlVGFyZ2V0S2luZFtdXG4gIGNyYXRlX3R5cGVzOiBDcmF0ZVRhcmdldEtpbmRbXVxufVxuXG5leHBvcnQgaW50ZXJmYWNlIENyYXRlIHtcbiAgaWQ6IHN0cmluZ1xuICBuYW1lOiBzdHJpbmdcbiAgc3JjX3BhdGg6IHN0cmluZ1xuICB2ZXJzaW9uOiBzdHJpbmdcbiAgZWRpdGlvbjogc3RyaW5nXG4gIHRhcmdldHM6IENyYXRlVGFyZ2V0W11cbiAgZmVhdHVyZXM6IFJlY29yZDxzdHJpbmcsIHN0cmluZ1tdPlxuICBtYW5pZmVzdF9wYXRoOiBzdHJpbmdcbiAgZGVwZW5kZW5jaWVzOiBBcnJheTx7XG4gICAgbmFtZTogc3RyaW5nXG4gICAgc291cmNlOiBzdHJpbmdcbiAgICByZXE6IHN0cmluZ1xuICAgIGtpbmQ6IHN0cmluZyB8IG51bGxcbiAgICByZW5hbWU6IHN0cmluZyB8IG51bGxcbiAgICBvcHRpb25hbDogYm9vbGVhblxuICAgIHVzZXNfZGVmYXVsdF9mZWF0dXJlczogYm9vbGVhblxuICAgIGZlYXR1cmVzOiBzdHJpbmdbXVxuICAgIHRhcmdldDogc3RyaW5nIHwgbnVsbFxuICAgIHJlZ2lzdHJ5OiBzdHJpbmcgfCBudWxsXG4gIH0+XG59XG5cbmV4cG9ydCBpbnRlcmZhY2UgQ2FyZ29Xb3Jrc3BhY2VNZXRhZGF0YSB7XG4gIHZlcnNpb246IG51bWJlclxuICBwYWNrYWdlczogQ3JhdGVbXVxuICB3b3Jrc3BhY2VfbWVtYmVyczogc3RyaW5nW11cbiAgdGFyZ2V0X2RpcmVjdG9yeTogc3RyaW5nXG4gIHdvcmtzcGFjZV9yb290OiBzdHJpbmdcbn1cblxuZXhwb3J0IGFzeW5jIGZ1bmN0aW9uIHBhcnNlTWV0YWRhdGEobWFuaWZlc3RQYXRoOiBzdHJpbmcpIHtcbiAgaWYgKCFmcy5leGlzdHNTeW5jKG1hbmlmZXN0UGF0aCkpIHtcbiAgICB0aHJvdyBuZXcgRXJyb3IoYE5vIGNyYXRlIGZvdW5kIGluIG1hbmlmZXN0OiAke21hbmlmZXN0UGF0aH1gKVxuICB9XG5cbiAgY29uc3QgY2hpbGRQcm9jZXNzID0gc3Bhd24oXG4gICAgJ2NhcmdvJyxcbiAgICBbJ21ldGFkYXRhJywgJy0tbWFuaWZlc3QtcGF0aCcsIG1hbmlmZXN0UGF0aCwgJy0tZm9ybWF0LXZlcnNpb24nLCAnMSddLFxuICAgIHsgc3RkaW86ICdwaXBlJyB9LFxuICApXG5cbiAgbGV0IHN0ZG91dCA9ICcnXG4gIGxldCBzdGRlcnIgPSAnJ1xuICBsZXQgc3RhdHVzID0gMFxuICBsZXQgZXJyb3IgPSBudWxsXG5cbiAgY2hpbGRQcm9jZXNzLnN0ZG91dC5vbignZGF0YScsIChkYXRhKSA9PiB7XG4gICAgc3Rkb3V0ICs9IGRhdGFcbiAgfSlcblxuICBjaGlsZFByb2Nlc3Muc3RkZXJyLm9uKCdkYXRhJywgKGRhdGEpID0+IHtcbiAgICBzdGRlcnIgKz0gZGF0YVxuICB9KVxuXG4gIGF3YWl0IG5ldyBQcm9taXNlPHZvaWQ+KChyZXNvbHZlKSA9PiB7XG4gICAgY2hpbGRQcm9jZXNzLm9uKCdjbG9zZScsIChjb2RlKSA9PiB7XG4gICAgICBzdGF0dXMgPSBjb2RlID8/IDBcbiAgICAgIHJlc29sdmUoKVxuICAgIH0pXG4gIH0pXG5cbiAgaWYgKGVycm9yKSB7XG4gICAgdGhyb3cgbmV3IEVycm9yKCdjYXJnbyBtZXRhZGF0YSBmYWlsZWQgdG8gcnVuJywgeyBjYXVzZTogZXJyb3IgfSlcbiAgfVxuICBpZiAoc3RhdHVzICE9PSAwKSB7XG4gICAgY29uc3Qgc2ltcGxlTWVzc2FnZSA9IGBjYXJnbyBtZXRhZGF0YSBleGl0ZWQgd2l0aCBjb2RlICR7c3RhdHVzfWBcbiAgICB0aHJvdyBuZXcgRXJyb3IoYCR7c2ltcGxlTWVzc2FnZX0gYW5kIGVycm9yIG1lc3NhZ2U6XFxuXFxuJHtzdGRlcnJ9YCwge1xuICAgICAgY2F1c2U6IG5ldyBFcnJvcihzaW1wbGVNZXNzYWdlKSxcbiAgICB9KVxuICB9XG5cbiAgdHJ5IHtcbiAgICByZXR1cm4gSlNPTi5wYXJzZShzdGRvdXQpIGFzIENhcmdvV29ya3NwYWNlTWV0YWRhdGFcbiAgfSBjYXRjaCAoZSkge1xuICAgIHRocm93IG5ldyBFcnJvcignRmFpbGVkIHRvIHBhcnNlIGNhcmdvIG1ldGFkYXRhIEpTT04nLCB7IGNhdXNlOiBlIH0pXG4gIH1cbn1cbiIsImltcG9ydCB7IHVuZGVybGluZSwgeWVsbG93IH0gZnJvbSAnY29sb3JldHRlJ1xuaW1wb3J0IHsgbWVyZ2UsIG9taXQgfSBmcm9tICdlcy10b29sa2l0J1xuXG5pbXBvcnQgeyBmaWxlRXhpc3RzLCByZWFkRmlsZUFzeW5jIH0gZnJvbSAnLi9taXNjLmpzJ1xuaW1wb3J0IHsgREVGQVVMVF9UQVJHRVRTLCBwYXJzZVRyaXBsZSwgdHlwZSBUYXJnZXQgfSBmcm9tICcuL3RhcmdldC5qcydcblxuZXhwb3J0IHR5cGUgVmFsdWVPZkNvbnN0QXJyYXk8VD4gPSBUW0V4Y2x1ZGU8a2V5b2YgVCwga2V5b2YgQXJyYXk8YW55Pj5dXG5cbmV4cG9ydCBjb25zdCBTdXBwb3J0ZWRQYWNrYWdlTWFuYWdlcnMgPSBbJ3lhcm4nLCAncG5wbSddIGFzIGNvbnN0XG5leHBvcnQgY29uc3QgU3VwcG9ydGVkVGVzdEZyYW1ld29ya3MgPSBbJ2F2YSddIGFzIGNvbnN0XG5cbmV4cG9ydCB0eXBlIFN1cHBvcnRlZFBhY2thZ2VNYW5hZ2VyID0gVmFsdWVPZkNvbnN0QXJyYXk8XG4gIHR5cGVvZiBTdXBwb3J0ZWRQYWNrYWdlTWFuYWdlcnNcbj5cbmV4cG9ydCB0eXBlIFN1cHBvcnRlZFRlc3RGcmFtZXdvcmsgPSBWYWx1ZU9mQ29uc3RBcnJheTxcbiAgdHlwZW9mIFN1cHBvcnRlZFRlc3RGcmFtZXdvcmtzXG4+XG5cbmV4cG9ydCBpbnRlcmZhY2UgVXNlck5hcGlDb25maWcge1xuICAvKipcbiAgICogTmFtZSBvZiB0aGUgYmluYXJ5IHRvIGJlIGdlbmVyYXRlZCwgZGVmYXVsdCB0byBgaW5kZXhgXG4gICAqL1xuICBiaW5hcnlOYW1lPzogc3RyaW5nXG4gIC8qKlxuICAgKiBOYW1lIG9mIHRoZSBucG0gcGFja2FnZSwgZGVmYXVsdCB0byB0aGUgbmFtZSBvZiByb290IHBhY2thZ2UuanNvbiBuYW1lXG4gICAqXG4gICAqIEFsd2F5cyBnaXZlbiBgQHNjb3BlL3BrZ2AgYW5kIGFyY2ggc3VmZml4IHdpbGwgYmUgYXBwZW5kZWQgbGlrZSBgQHNjb3BlL3BrZy1saW51eC1nbnUteDY0YFxuICAgKi9cbiAgcGFja2FnZU5hbWU/OiBzdHJpbmdcbiAgLyoqXG4gICAqIEFsbCB0YXJnZXRzIHRoZSBjcmF0ZSB3aWxsIGJlIGNvbXBpbGVkIGZvclxuICAgKi9cbiAgdGFyZ2V0cz86IHN0cmluZ1tdXG5cbiAgLyoqXG4gICAqIFRoZSBucG0gY2xpZW50IHByb2plY3QgdXNlcy5cbiAgICovXG4gIG5wbUNsaWVudD86IHN0cmluZ1xuXG4gIC8qKlxuICAgKiBXaGV0aGVyIGdlbmVyYXRlIGNvbnN0IGVudW0gZm9yIHR5cGVzY3JpcHQgYmluZGluZ3NcbiAgICovXG4gIGNvbnN0RW51bT86IGJvb2xlYW5cblxuICAvKipcbiAgICogZHRzIGhlYWRlciBwcmVwZW5kIHRvIHRoZSBnZW5lcmF0ZWQgZHRzIGZpbGVcbiAgICovXG4gIGR0c0hlYWRlcj86IHN0cmluZ1xuXG4gIC8qKlxuICAgKiBkdHMgaGVhZGVyIGZpbGUgcGF0aCB0byBiZSBwcmVwZW5kZWQgdG8gdGhlIGdlbmVyYXRlZCBkdHMgZmlsZVxuICAgKiBpZiBib3RoIGR0c0hlYWRlciBhbmQgZHRzSGVhZGVyRmlsZSBhcmUgcHJvdmlkZWQsIGR0c0hlYWRlckZpbGUgd2lsbCBiZSB1c2VkXG4gICAqL1xuICBkdHNIZWFkZXJGaWxlPzogc3RyaW5nXG5cbiAgLyoqXG4gICAqIHdhc20gY29tcGlsYXRpb24gb3B0aW9uc1xuICAgKi9cbiAgd2FzbT86IHtcbiAgICAvKipcbiAgICAgKiBodHRwczovL2RldmVsb3Blci5tb3ppbGxhLm9yZy9lbi1VUy9kb2NzL1dlYkFzc2VtYmx5L0phdmFTY3JpcHRfaW50ZXJmYWNlL01lbW9yeVxuICAgICAqIEBkZWZhdWx0IDQwMDAgcGFnZXMgKDI1Nk1pQilcbiAgICAgKi9cbiAgICBpbml0aWFsTWVtb3J5PzogbnVtYmVyXG4gICAgLyoqXG4gICAgICogQGRlZmF1bHQgNjU1MzYgcGFnZXMgKDRHaUIpXG4gICAgICovXG4gICAgbWF4aW11bU1lbW9yeT86IG51bWJlclxuXG4gICAgLyoqXG4gICAgICogQnJvd3NlciB3YXNtIGJpbmRpbmcgY29uZmlndXJhdGlvblxuICAgICAqL1xuICAgIGJyb3dzZXI6IHtcbiAgICAgIC8qKlxuICAgICAgICogV2hldGhlciB0byB1c2UgZnMgbW9kdWxlIGluIGJyb3dzZXJcbiAgICAgICAqL1xuICAgICAgZnM/OiBib29sZWFuXG4gICAgICAvKipcbiAgICAgICAqIFdoZXRoZXIgdG8gaW5pdGlhbGl6ZSB3YXNtIGFzeW5jaHJvbm91c2x5XG4gICAgICAgKi9cbiAgICAgIGFzeW5jSW5pdD86IGJvb2xlYW5cbiAgICAgIC8qKlxuICAgICAgICogV2hldGhlciB0byBpbmplY3QgYGJ1ZmZlcmAgdG8gZW1uYXBpIGNvbnRleHRcbiAgICAgICAqL1xuICAgICAgYnVmZmVyPzogYm9vbGVhblxuICAgIH1cbiAgfVxuXG4gIC8qKlxuICAgKiBAZGVwcmVjYXRlZCBiaW5hcnlOYW1lIGluc3RlYWRcbiAgICovXG4gIG5hbWU/OiBzdHJpbmdcbiAgLyoqXG4gICAqIEBkZXByZWNhdGVkIHVzZSBwYWNrYWdlTmFtZSBpbnN0ZWFkXG4gICAqL1xuICBwYWNrYWdlPzoge1xuICAgIG5hbWU/OiBzdHJpbmdcbiAgfVxuICAvKipcbiAgICogQGRlcHJlY2F0ZWQgdXNlIHRhcmdldHMgaW5zdGVhZFxuICAgKi9cbiAgdHJpcGxlcz86IHtcbiAgICAvKipcbiAgICAgKiBXaGV0aGVyIGVuYWJsZSBkZWZhdWx0IHRhcmdldHNcbiAgICAgKi9cbiAgICBkZWZhdWx0czogYm9vbGVhblxuICAgIC8qKlxuICAgICAqIEFkZGl0aW9uYWwgdGFyZ2V0cyB0byBiZSBjb21waWxlZCBmb3JcbiAgICAgKi9cbiAgICBhZGRpdGlvbmFsPzogc3RyaW5nW11cbiAgfVxufVxuXG5leHBvcnQgaW50ZXJmYWNlIENvbW1vblBhY2thZ2VKc29uRmllbGRzIHtcbiAgbmFtZTogc3RyaW5nXG4gIHZlcnNpb246IHN0cmluZ1xuICBkZXNjcmlwdGlvbj86IHN0cmluZ1xuICBrZXl3b3Jkcz86IHN0cmluZ1tdXG4gIGF1dGhvcj86IHN0cmluZ1xuICBhdXRob3JzPzogc3RyaW5nW11cbiAgbGljZW5zZT86IHN0cmluZ1xuICBjcHU/OiBzdHJpbmdbXVxuICBvcz86IHN0cmluZ1tdXG4gIGxpYmM/OiBzdHJpbmdbXVxuICBmaWxlcz86IHN0cmluZ1tdXG4gIHJlcG9zaXRvcnk/OiBhbnlcbiAgaG9tZXBhZ2U/OiBhbnlcbiAgZW5naW5lcz86IFJlY29yZDxzdHJpbmcsIHN0cmluZz5cbiAgcHVibGlzaENvbmZpZz86IGFueVxuICBidWdzPzogYW55XG4gIC8vIGVzbGludC1kaXNhYmxlLW5leHQtbGluZSBuby11c2UtYmVmb3JlLWRlZmluZVxuICBuYXBpPzogVXNlck5hcGlDb25maWdcbiAgdHlwZT86ICdtb2R1bGUnIHwgJ2NvbW1vbmpzJ1xuICBzY3JpcHRzPzogUmVjb3JkPHN0cmluZywgc3RyaW5nPlxuXG4gIC8vIG1vZHVsZXNcbiAgbWFpbj86IHN0cmluZ1xuICBtb2R1bGU/OiBzdHJpbmdcbiAgdHlwZXM/OiBzdHJpbmdcbiAgYnJvd3Nlcj86IHN0cmluZ1xuICBleHBvcnRzPzogYW55XG5cbiAgZGVwZW5kZW5jaWVzPzogUmVjb3JkPHN0cmluZywgc3RyaW5nPlxuICBkZXZEZXBlbmRlbmNpZXM/OiBSZWNvcmQ8c3RyaW5nLCBzdHJpbmc+XG5cbiAgYXZhPzoge1xuICAgIHRpbWVvdXQ/OiBzdHJpbmdcbiAgfVxufVxuXG5leHBvcnQgdHlwZSBOYXBpQ29uZmlnID0gUmVxdWlyZWQ8XG4gIFBpY2s8VXNlck5hcGlDb25maWcsICdiaW5hcnlOYW1lJyB8ICdwYWNrYWdlTmFtZScgfCAnbnBtQ2xpZW50Jz5cbj4gJlxuICBQaWNrPFVzZXJOYXBpQ29uZmlnLCAnd2FzbScgfCAnZHRzSGVhZGVyJyB8ICdkdHNIZWFkZXJGaWxlJyB8ICdjb25zdEVudW0nPiAmIHtcbiAgICB0YXJnZXRzOiBUYXJnZXRbXVxuICAgIHBhY2thZ2VKc29uOiBDb21tb25QYWNrYWdlSnNvbkZpZWxkc1xuICB9XG5cbmV4cG9ydCBhc3luYyBmdW5jdGlvbiByZWFkTmFwaUNvbmZpZyhcbiAgcGF0aDogc3RyaW5nLFxuICBjb25maWdQYXRoPzogc3RyaW5nLFxuKTogUHJvbWlzZTxOYXBpQ29uZmlnPiB7XG4gIGlmIChjb25maWdQYXRoICYmICEoYXdhaXQgZmlsZUV4aXN0cyhjb25maWdQYXRoKSkpIHtcbiAgICB0aHJvdyBuZXcgRXJyb3IoYE5BUEktUlMgY29uZmlnIG5vdCBmb3VuZCBhdCAke2NvbmZpZ1BhdGh9YClcbiAgfVxuICBpZiAoIShhd2FpdCBmaWxlRXhpc3RzKHBhdGgpKSkge1xuICAgIHRocm93IG5ldyBFcnJvcihgcGFja2FnZS5qc29uIG5vdCBmb3VuZCBhdCAke3BhdGh9YClcbiAgfVxuICAvLyBNYXkgc3VwcG9ydCBtdWx0aXBsZSBjb25maWcgc291cmNlcyBsYXRlciBvbi5cbiAgY29uc3QgY29udGVudCA9IGF3YWl0IHJlYWRGaWxlQXN5bmMocGF0aCwgJ3V0ZjgnKVxuICBsZXQgcGtnSnNvblxuICB0cnkge1xuICAgIHBrZ0pzb24gPSBKU09OLnBhcnNlKGNvbnRlbnQpIGFzIENvbW1vblBhY2thZ2VKc29uRmllbGRzXG4gIH0gY2F0Y2ggKGUpIHtcbiAgICB0aHJvdyBuZXcgRXJyb3IoYEZhaWxlZCB0byBwYXJzZSBwYWNrYWdlLmpzb24gYXQgJHtwYXRofWAsIHtcbiAgICAgIGNhdXNlOiBlLFxuICAgIH0pXG4gIH1cblxuICBsZXQgc2VwYXJhdGVkQ29uZmlnOiBVc2VyTmFwaUNvbmZpZyB8IHVuZGVmaW5lZFxuICBpZiAoY29uZmlnUGF0aCkge1xuICAgIGNvbnN0IGNvbmZpZ0NvbnRlbnQgPSBhd2FpdCByZWFkRmlsZUFzeW5jKGNvbmZpZ1BhdGgsICd1dGY4JylcbiAgICB0cnkge1xuICAgICAgc2VwYXJhdGVkQ29uZmlnID0gSlNPTi5wYXJzZShjb25maWdDb250ZW50KSBhcyBVc2VyTmFwaUNvbmZpZ1xuICAgIH0gY2F0Y2ggKGUpIHtcbiAgICAgIHRocm93IG5ldyBFcnJvcihgRmFpbGVkIHRvIHBhcnNlIE5BUEktUlMgY29uZmlnIGF0ICR7Y29uZmlnUGF0aH1gLCB7XG4gICAgICAgIGNhdXNlOiBlLFxuICAgICAgfSlcbiAgICB9XG4gIH1cblxuICBjb25zdCB1c2VyTmFwaUNvbmZpZyA9IHBrZ0pzb24ubmFwaSA/PyB7fVxuICBpZiAocGtnSnNvbi5uYXBpICYmIHNlcGFyYXRlZENvbmZpZykge1xuICAgIGNvbnN0IHBrZ0pzb25QYXRoID0gdW5kZXJsaW5lKHBhdGgpXG4gICAgY29uc3QgY29uZmlnUGF0aFVuZGVybGluZSA9IHVuZGVybGluZShjb25maWdQYXRoISlcbiAgICBjb25zb2xlLndhcm4oXG4gICAgICB5ZWxsb3coXG4gICAgICAgIGBCb3RoIG5hcGkgZmllbGQgaW4gJHtwa2dKc29uUGF0aH0gYW5kIFtOQVBJLVJTIGNvbmZpZ10oJHtjb25maWdQYXRoVW5kZXJsaW5lfSkgZmlsZSBhcmUgZm91bmQsIHRoZSBOQVBJLVJTIGNvbmZpZyBmaWxlIHdpbGwgYmUgdXNlZC5gLFxuICAgICAgKSxcbiAgICApXG4gIH1cbiAgaWYgKHNlcGFyYXRlZENvbmZpZykge1xuICAgIE9iamVjdC5hc3NpZ24odXNlck5hcGlDb25maWcsIHNlcGFyYXRlZENvbmZpZylcbiAgfVxuICBjb25zdCBuYXBpQ29uZmlnOiBOYXBpQ29uZmlnID0gbWVyZ2UoXG4gICAge1xuICAgICAgYmluYXJ5TmFtZTogJ2luZGV4JyxcbiAgICAgIHBhY2thZ2VOYW1lOiBwa2dKc29uLm5hbWUsXG4gICAgICB0YXJnZXRzOiBbXSxcbiAgICAgIHBhY2thZ2VKc29uOiBwa2dKc29uLFxuICAgICAgbnBtQ2xpZW50OiAnbnBtJyxcbiAgICB9LFxuICAgIG9taXQodXNlck5hcGlDb25maWcsIFsndGFyZ2V0cyddKSxcbiAgKVxuXG4gIGxldCB0YXJnZXRzOiBzdHJpbmdbXSA9IHVzZXJOYXBpQ29uZmlnLnRhcmdldHMgPz8gW11cblxuICAvLyBjb21wYXRpYmxlIHdpdGggb2xkIGNvbmZpZ1xuICBpZiAodXNlck5hcGlDb25maWc/Lm5hbWUpIHtcbiAgICBjb25zb2xlLndhcm4oXG4gICAgICB5ZWxsb3coXG4gICAgICAgIGBbREVQUkVDQVRFRF0gbmFwaS5uYW1lIGlzIGRlcHJlY2F0ZWQsIHVzZSBuYXBpLmJpbmFyeU5hbWUgaW5zdGVhZC5gLFxuICAgICAgKSxcbiAgICApXG4gICAgbmFwaUNvbmZpZy5iaW5hcnlOYW1lID0gdXNlck5hcGlDb25maWcubmFtZVxuICB9XG5cbiAgaWYgKCF0YXJnZXRzLmxlbmd0aCkge1xuICAgIGxldCBkZXByZWNhdGVkV2FybmVkID0gZmFsc2VcbiAgICBjb25zdCB3YXJuaW5nID0geWVsbG93KFxuICAgICAgYFtERVBSRUNBVEVEXSBuYXBpLnRyaXBsZXMgaXMgZGVwcmVjYXRlZCwgdXNlIG5hcGkudGFyZ2V0cyBpbnN0ZWFkLmAsXG4gICAgKVxuICAgIGlmICh1c2VyTmFwaUNvbmZpZy50cmlwbGVzPy5kZWZhdWx0cykge1xuICAgICAgZGVwcmVjYXRlZFdhcm5lZCA9IHRydWVcbiAgICAgIGNvbnNvbGUud2Fybih3YXJuaW5nKVxuICAgICAgdGFyZ2V0cyA9IHRhcmdldHMuY29uY2F0KERFRkFVTFRfVEFSR0VUUylcbiAgICB9XG5cbiAgICBpZiAodXNlck5hcGlDb25maWcudHJpcGxlcz8uYWRkaXRpb25hbD8ubGVuZ3RoKSB7XG4gICAgICB0YXJnZXRzID0gdGFyZ2V0cy5jb25jYXQodXNlck5hcGlDb25maWcudHJpcGxlcy5hZGRpdGlvbmFsKVxuICAgICAgaWYgKCFkZXByZWNhdGVkV2FybmVkKSB7XG4gICAgICAgIGNvbnNvbGUud2Fybih3YXJuaW5nKVxuICAgICAgfVxuICAgIH1cbiAgfVxuXG4gIC8vIGZpbmQgZHVwbGljYXRlIHRhcmdldHNcbiAgY29uc3QgdW5pcXVlVGFyZ2V0cyA9IG5ldyBTZXQodGFyZ2V0cylcbiAgaWYgKHVuaXF1ZVRhcmdldHMuc2l6ZSAhPT0gdGFyZ2V0cy5sZW5ndGgpIHtcbiAgICBjb25zdCBkdXBsaWNhdGVUYXJnZXQgPSB0YXJnZXRzLmZpbmQoXG4gICAgICAodGFyZ2V0LCBpbmRleCkgPT4gdGFyZ2V0cy5pbmRleE9mKHRhcmdldCkgIT09IGluZGV4LFxuICAgIClcbiAgICB0aHJvdyBuZXcgRXJyb3IoYER1cGxpY2F0ZSB0YXJnZXRzIGFyZSBub3QgYWxsb3dlZDogJHtkdXBsaWNhdGVUYXJnZXR9YClcbiAgfVxuXG4gIG5hcGlDb25maWcudGFyZ2V0cyA9IHRhcmdldHMubWFwKHBhcnNlVHJpcGxlKVxuXG4gIHJldHVybiBuYXBpQ29uZmlnXG59XG4iLCJpbXBvcnQgeyBleGVjU3luYyB9IGZyb20gJ25vZGU6Y2hpbGRfcHJvY2VzcydcblxuaW1wb3J0IHsgZGVidWcgfSBmcm9tICcuL2xvZy5qcydcblxuZXhwb3J0IGZ1bmN0aW9uIHRyeUluc3RhbGxDYXJnb0JpbmFyeShuYW1lOiBzdHJpbmcsIGJpbjogc3RyaW5nKSB7XG4gIGlmIChkZXRlY3RDYXJnb0JpbmFyeShiaW4pKSB7XG4gICAgZGVidWcoJ0NhcmdvIGJpbmFyeSBhbHJlYWR5IGluc3RhbGxlZDogJXMnLCBuYW1lKVxuICAgIHJldHVyblxuICB9XG5cbiAgdHJ5IHtcbiAgICBkZWJ1ZygnSW5zdGFsbGluZyBjYXJnbyBiaW5hcnk6ICVzJywgbmFtZSlcbiAgICBleGVjU3luYyhgY2FyZ28gaW5zdGFsbCAke25hbWV9YCwge1xuICAgICAgc3RkaW86ICdpbmhlcml0JyxcbiAgICB9KVxuICB9IGNhdGNoIChlKSB7XG4gICAgdGhyb3cgbmV3IEVycm9yKGBGYWlsZWQgdG8gaW5zdGFsbCBjYXJnbyBiaW5hcnk6ICR7bmFtZX1gLCB7XG4gICAgICBjYXVzZTogZSxcbiAgICB9KVxuICB9XG59XG5cbmZ1bmN0aW9uIGRldGVjdENhcmdvQmluYXJ5KGJpbjogc3RyaW5nKSB7XG4gIGRlYnVnKCdEZXRlY3RpbmcgY2FyZ28gYmluYXJ5OiAlcycsIGJpbilcbiAgdHJ5IHtcbiAgICBleGVjU3luYyhgY2FyZ28gaGVscCAke2Jpbn1gLCB7XG4gICAgICBzdGRpbzogJ2lnbm9yZScsXG4gICAgfSlcbiAgICBkZWJ1ZygnQ2FyZ28gYmluYXJ5IGRldGVjdGVkOiAlcycsIGJpbilcbiAgICByZXR1cm4gdHJ1ZVxuICB9IGNhdGNoIHtcbiAgICBkZWJ1ZygnQ2FyZ28gYmluYXJ5IG5vdCBkZXRlY3RlZDogJXMnLCBiaW4pXG4gICAgcmV0dXJuIGZhbHNlXG4gIH1cbn1cbiIsImltcG9ydCB7IHNvcnRCeSB9IGZyb20gJ2VzLXRvb2xraXQnXG5cbmltcG9ydCB7IHJlYWRGaWxlQXN5bmMgfSBmcm9tICcuL21pc2MuanMnXG5cbmNvbnN0IFRPUF9MRVZFTF9OQU1FU1BBQ0UgPSAnX19UT1BfTEVWRUxfTU9EVUxFX18nXG5leHBvcnQgY29uc3QgREVGQVVMVF9UWVBFX0RFRl9IRUFERVIgPSBgLyogYXV0by1nZW5lcmF0ZWQgYnkgTkFQSS1SUyAqL1xuLyogZXNsaW50LWRpc2FibGUgKi9cbmBcblxuZW51bSBUeXBlRGVmS2luZCB7XG4gIENvbnN0ID0gJ2NvbnN0JyxcbiAgRW51bSA9ICdlbnVtJyxcbiAgU3RyaW5nRW51bSA9ICdzdHJpbmdfZW51bScsXG4gIEludGVyZmFjZSA9ICdpbnRlcmZhY2UnLFxuICBUeXBlID0gJ3R5cGUnLFxuICBGbiA9ICdmbicsXG4gIFN0cnVjdCA9ICdzdHJ1Y3QnLFxuICBFeHRlbmRzID0gJ2V4dGVuZHMnLFxuICBJbXBsID0gJ2ltcGwnLFxufVxuXG5pbnRlcmZhY2UgVHlwZURlZkxpbmUge1xuICBraW5kOiBUeXBlRGVmS2luZFxuICBuYW1lOiBzdHJpbmdcbiAgb3JpZ2luYWxfbmFtZT86IHN0cmluZ1xuICBkZWY6IHN0cmluZ1xuICBleHRlbmRzPzogc3RyaW5nXG4gIGpzX2RvYz86IHN0cmluZ1xuICBqc19tb2Q/OiBzdHJpbmdcbn1cblxuZnVuY3Rpb24gcHJldHR5UHJpbnQoXG4gIGxpbmU6IFR5cGVEZWZMaW5lLFxuICBjb25zdEVudW06IGJvb2xlYW4sXG4gIGlkZW50OiBudW1iZXIsXG4gIGFtYmllbnQgPSBmYWxzZSxcbik6IHN0cmluZyB7XG4gIGxldCBzID0gbGluZS5qc19kb2MgPz8gJydcbiAgc3dpdGNoIChsaW5lLmtpbmQpIHtcbiAgICBjYXNlIFR5cGVEZWZLaW5kLkludGVyZmFjZTpcbiAgICAgIHMgKz0gYGV4cG9ydCBpbnRlcmZhY2UgJHtsaW5lLm5hbWV9IHtcXG4ke2xpbmUuZGVmfVxcbn1gXG4gICAgICBicmVha1xuXG4gICAgY2FzZSBUeXBlRGVmS2luZC5UeXBlOlxuICAgICAgcyArPSBgZXhwb3J0IHR5cGUgJHtsaW5lLm5hbWV9ID0gXFxuJHtsaW5lLmRlZn1gXG4gICAgICBicmVha1xuXG4gICAgY2FzZSBUeXBlRGVmS2luZC5FbnVtOlxuICAgICAgY29uc3QgZW51bU5hbWUgPSBjb25zdEVudW0gPyAnY29uc3QgZW51bScgOiAnZW51bSdcbiAgICAgIHMgKz0gYCR7ZXhwb3J0RGVjbGFyZShhbWJpZW50KX0gJHtlbnVtTmFtZX0gJHtsaW5lLm5hbWV9IHtcXG4ke2xpbmUuZGVmfVxcbn1gXG4gICAgICBicmVha1xuXG4gICAgY2FzZSBUeXBlRGVmS2luZC5TdHJpbmdFbnVtOlxuICAgICAgaWYgKGNvbnN0RW51bSkge1xuICAgICAgICBzICs9IGAke2V4cG9ydERlY2xhcmUoYW1iaWVudCl9IGNvbnN0IGVudW0gJHtsaW5lLm5hbWV9IHtcXG4ke2xpbmUuZGVmfVxcbn1gXG4gICAgICB9IGVsc2Uge1xuICAgICAgICBzICs9IGBleHBvcnQgdHlwZSAke2xpbmUubmFtZX0gPSAke2xpbmUuZGVmLnJlcGxhY2VBbGwoLy4qPS9nLCAnJykucmVwbGFjZUFsbCgnLCcsICd8Jyl9O2BcbiAgICAgIH1cbiAgICAgIGJyZWFrXG5cbiAgICBjYXNlIFR5cGVEZWZLaW5kLlN0cnVjdDpcbiAgICAgIGNvbnN0IGV4dGVuZHNEZWYgPSBsaW5lLmV4dGVuZHMgPyBgIGV4dGVuZHMgJHtsaW5lLmV4dGVuZHN9YCA6ICcnXG4gICAgICBpZiAobGluZS5leHRlbmRzKSB7XG4gICAgICAgIC8vIEV4dHJhY3QgZ2VuZXJpYyBwYXJhbXMgZnJvbSBleHRlbmRzIHR5cGUgbGlrZSBJdGVyYXRvcjxULCBUUmVzdWx0LCBUTmV4dD5cbiAgICAgICAgY29uc3QgZ2VuZXJpY01hdGNoID0gbGluZS5leHRlbmRzLm1hdGNoKC9JdGVyYXRvcjwoLispPiQvKVxuICAgICAgICBpZiAoZ2VuZXJpY01hdGNoKSB7XG4gICAgICAgICAgY29uc3QgW1QsIFRSZXN1bHQsIFROZXh0XSA9IGdlbmVyaWNNYXRjaFsxXVxuICAgICAgICAgICAgLnNwbGl0KCcsJylcbiAgICAgICAgICAgIC5tYXAoKHApID0+IHAudHJpbSgpKVxuICAgICAgICAgIGxpbmUuZGVmID1cbiAgICAgICAgICAgIGxpbmUuZGVmICtcbiAgICAgICAgICAgIGBcXG5uZXh0KHZhbHVlPzogJHtUTmV4dH0pOiBJdGVyYXRvclJlc3VsdDwke1R9LCAke1RSZXN1bHR9PmBcbiAgICAgICAgfVxuICAgICAgfVxuICAgICAgcyArPSBgJHtleHBvcnREZWNsYXJlKGFtYmllbnQpfSBjbGFzcyAke2xpbmUubmFtZX0ke2V4dGVuZHNEZWZ9IHtcXG4ke2xpbmUuZGVmfVxcbn1gXG4gICAgICBpZiAobGluZS5vcmlnaW5hbF9uYW1lICYmIGxpbmUub3JpZ2luYWxfbmFtZSAhPT0gbGluZS5uYW1lKSB7XG4gICAgICAgIHMgKz0gYFxcbmV4cG9ydCB0eXBlICR7bGluZS5vcmlnaW5hbF9uYW1lfSA9ICR7bGluZS5uYW1lfWBcbiAgICAgIH1cbiAgICAgIGJyZWFrXG5cbiAgICBjYXNlIFR5cGVEZWZLaW5kLkZuOlxuICAgICAgcyArPSBgJHtleHBvcnREZWNsYXJlKGFtYmllbnQpfSAke2xpbmUuZGVmfWBcbiAgICAgIGJyZWFrXG5cbiAgICBkZWZhdWx0OlxuICAgICAgcyArPSBsaW5lLmRlZlxuICB9XG5cbiAgcmV0dXJuIGNvcnJlY3RTdHJpbmdJZGVudChzLCBpZGVudClcbn1cblxuZnVuY3Rpb24gZXhwb3J0RGVjbGFyZShhbWJpZW50OiBib29sZWFuKTogc3RyaW5nIHtcbiAgaWYgKGFtYmllbnQpIHtcbiAgICByZXR1cm4gJ2V4cG9ydCdcbiAgfVxuXG4gIHJldHVybiAnZXhwb3J0IGRlY2xhcmUnXG59XG5cbmV4cG9ydCBhc3luYyBmdW5jdGlvbiBwcm9jZXNzVHlwZURlZihcbiAgaW50ZXJtZWRpYXRlVHlwZUZpbGU6IHN0cmluZyxcbiAgY29uc3RFbnVtOiBib29sZWFuLFxuKSB7XG4gIGNvbnN0IGV4cG9ydHM6IHN0cmluZ1tdID0gW11cbiAgY29uc3QgZGVmcyA9IGF3YWl0IHJlYWRJbnRlcm1lZGlhdGVUeXBlRmlsZShpbnRlcm1lZGlhdGVUeXBlRmlsZSlcbiAgY29uc3QgZ3JvdXBlZERlZnMgPSBwcmVwcm9jZXNzVHlwZURlZihkZWZzKVxuXG4gIGNvbnN0IGR0cyA9XG4gICAgc29ydEJ5KEFycmF5LmZyb20oZ3JvdXBlZERlZnMpLCBbKFtuYW1lc3BhY2VdKSA9PiBuYW1lc3BhY2VdKVxuICAgICAgLm1hcCgoW25hbWVzcGFjZSwgZGVmc10pID0+IHtcbiAgICAgICAgaWYgKG5hbWVzcGFjZSA9PT0gVE9QX0xFVkVMX05BTUVTUEFDRSkge1xuICAgICAgICAgIHJldHVybiBkZWZzXG4gICAgICAgICAgICAubWFwKChkZWYpID0+IHtcbiAgICAgICAgICAgICAgc3dpdGNoIChkZWYua2luZCkge1xuICAgICAgICAgICAgICAgIGNhc2UgVHlwZURlZktpbmQuQ29uc3Q6XG4gICAgICAgICAgICAgICAgY2FzZSBUeXBlRGVmS2luZC5FbnVtOlxuICAgICAgICAgICAgICAgIGNhc2UgVHlwZURlZktpbmQuU3RyaW5nRW51bTpcbiAgICAgICAgICAgICAgICBjYXNlIFR5cGVEZWZLaW5kLkZuOlxuICAgICAgICAgICAgICAgIGNhc2UgVHlwZURlZktpbmQuU3RydWN0OiB7XG4gICAgICAgICAgICAgICAgICBleHBvcnRzLnB1c2goZGVmLm5hbWUpXG4gICAgICAgICAgICAgICAgICBpZiAoZGVmLm9yaWdpbmFsX25hbWUgJiYgZGVmLm9yaWdpbmFsX25hbWUgIT09IGRlZi5uYW1lKSB7XG4gICAgICAgICAgICAgICAgICAgIGV4cG9ydHMucHVzaChkZWYub3JpZ2luYWxfbmFtZSlcbiAgICAgICAgICAgICAgICAgIH1cbiAgICAgICAgICAgICAgICAgIGJyZWFrXG4gICAgICAgICAgICAgICAgfVxuICAgICAgICAgICAgICAgIGRlZmF1bHQ6XG4gICAgICAgICAgICAgICAgICBicmVha1xuICAgICAgICAgICAgICB9XG4gICAgICAgICAgICAgIHJldHVybiBwcmV0dHlQcmludChkZWYsIGNvbnN0RW51bSwgMClcbiAgICAgICAgICAgIH0pXG4gICAgICAgICAgICAuam9pbignXFxuXFxuJylcbiAgICAgICAgfSBlbHNlIHtcbiAgICAgICAgICBleHBvcnRzLnB1c2gobmFtZXNwYWNlKVxuICAgICAgICAgIGxldCBkZWNsYXJhdGlvbiA9ICcnXG4gICAgICAgICAgZGVjbGFyYXRpb24gKz0gYGV4cG9ydCBkZWNsYXJlIG5hbWVzcGFjZSAke25hbWVzcGFjZX0ge1xcbmBcbiAgICAgICAgICBmb3IgKGNvbnN0IGRlZiBvZiBkZWZzKSB7XG4gICAgICAgICAgICBkZWNsYXJhdGlvbiArPSBwcmV0dHlQcmludChkZWYsIGNvbnN0RW51bSwgMiwgdHJ1ZSkgKyAnXFxuJ1xuICAgICAgICAgIH1cbiAgICAgICAgICBkZWNsYXJhdGlvbiArPSAnfSdcbiAgICAgICAgICByZXR1cm4gZGVjbGFyYXRpb25cbiAgICAgICAgfVxuICAgICAgfSlcbiAgICAgIC5qb2luKCdcXG5cXG4nKSArICdcXG4nXG5cbiAgcmV0dXJuIHtcbiAgICBkdHMsXG4gICAgZXhwb3J0cyxcbiAgfVxufVxuXG5hc3luYyBmdW5jdGlvbiByZWFkSW50ZXJtZWRpYXRlVHlwZUZpbGUoZmlsZTogc3RyaW5nKSB7XG4gIGNvbnN0IGNvbnRlbnQgPSBhd2FpdCByZWFkRmlsZUFzeW5jKGZpbGUsICd1dGY4JylcblxuICBjb25zdCBkZWZzID0gY29udGVudFxuICAgIC5zcGxpdCgnXFxuJylcbiAgICAuZmlsdGVyKEJvb2xlYW4pXG4gICAgLm1hcCgobGluZSkgPT4ge1xuICAgICAgbGluZSA9IGxpbmUudHJpbSgpXG4gICAgICBjb25zdCBwYXJzZWQgPSBKU09OLnBhcnNlKGxpbmUpIGFzIFR5cGVEZWZMaW5lXG4gICAgICAvLyBDb252ZXJ0IGVzY2FwZWQgbmV3bGluZXMgYmFjayB0byBhY3R1YWwgbmV3bGluZXMgaW4ganNfZG9jIGZpZWxkc1xuICAgICAgaWYgKHBhcnNlZC5qc19kb2MpIHtcbiAgICAgICAgcGFyc2VkLmpzX2RvYyA9IHBhcnNlZC5qc19kb2MucmVwbGFjZSgvXFxcXG4vZywgJ1xcbicpXG4gICAgICB9XG4gICAgICAvLyBDb252ZXJ0IGVzY2FwZWQgbmV3bGluZXMgdG8gYWN0dWFsIG5ld2xpbmVzIGluIGRlZiBmaWVsZHMgZm9yIHN0cnVjdC9jbGFzcy9pbnRlcmZhY2UvdHlwZSB0eXBlc1xuICAgICAgLy8gd2hlcmUgXFxuIHJlcHJlc2VudHMgbWV0aG9kL2ZpZWxkIHNlcGFyYXRvcnMgdGhhdCBzaG91bGQgYmUgYWN0dWFsIG5ld2xpbmVzXG4gICAgICBpZiAocGFyc2VkLmRlZikge1xuICAgICAgICBwYXJzZWQuZGVmID0gcGFyc2VkLmRlZi5yZXBsYWNlKC9cXFxcbi9nLCAnXFxuJylcbiAgICAgIH1cbiAgICAgIHJldHVybiBwYXJzZWRcbiAgICB9KVxuXG4gIC8vIG1vdmUgYWxsIGBzdHJ1Y3RgIGRlZiB0byB0aGUgdmVyeSB0b3BcbiAgLy8gYW5kIG9yZGVyIHRoZSByZXN0IGFscGhhYmV0aWNhbGx5LlxuICByZXR1cm4gZGVmcy5zb3J0KChhLCBiKSA9PiB7XG4gICAgaWYgKGEua2luZCA9PT0gVHlwZURlZktpbmQuU3RydWN0KSB7XG4gICAgICBpZiAoYi5raW5kID09PSBUeXBlRGVmS2luZC5TdHJ1Y3QpIHtcbiAgICAgICAgcmV0dXJuIGEubmFtZS5sb2NhbGVDb21wYXJlKGIubmFtZSlcbiAgICAgIH1cbiAgICAgIHJldHVybiAtMVxuICAgIH0gZWxzZSBpZiAoYi5raW5kID09PSBUeXBlRGVmS2luZC5TdHJ1Y3QpIHtcbiAgICAgIHJldHVybiAxXG4gICAgfSBlbHNlIHtcbiAgICAgIHJldHVybiBhLm5hbWUubG9jYWxlQ29tcGFyZShiLm5hbWUpXG4gICAgfVxuICB9KVxufVxuXG5mdW5jdGlvbiBwcmVwcm9jZXNzVHlwZURlZihkZWZzOiBUeXBlRGVmTGluZVtdKTogTWFwPHN0cmluZywgVHlwZURlZkxpbmVbXT4ge1xuICBjb25zdCBuYW1lc3BhY2VHcm91cGVkID0gbmV3IE1hcDxzdHJpbmcsIFR5cGVEZWZMaW5lW10+KClcbiAgY29uc3QgY2xhc3NEZWZzID0gbmV3IE1hcDxzdHJpbmcsIFR5cGVEZWZMaW5lPigpXG5cbiAgZm9yIChjb25zdCBkZWYgb2YgZGVmcykge1xuICAgIGNvbnN0IG5hbWVzcGFjZSA9IGRlZi5qc19tb2QgPz8gVE9QX0xFVkVMX05BTUVTUEFDRVxuICAgIGlmICghbmFtZXNwYWNlR3JvdXBlZC5oYXMobmFtZXNwYWNlKSkge1xuICAgICAgbmFtZXNwYWNlR3JvdXBlZC5zZXQobmFtZXNwYWNlLCBbXSlcbiAgICB9XG5cbiAgICBjb25zdCBncm91cCA9IG5hbWVzcGFjZUdyb3VwZWQuZ2V0KG5hbWVzcGFjZSkhXG5cbiAgICBpZiAoZGVmLmtpbmQgPT09IFR5cGVEZWZLaW5kLlN0cnVjdCkge1xuICAgICAgZ3JvdXAucHVzaChkZWYpXG4gICAgICBjbGFzc0RlZnMuc2V0KGRlZi5uYW1lLCBkZWYpXG4gICAgfSBlbHNlIGlmIChkZWYua2luZCA9PT0gVHlwZURlZktpbmQuRXh0ZW5kcykge1xuICAgICAgY29uc3QgY2xhc3NEZWYgPSBjbGFzc0RlZnMuZ2V0KGRlZi5uYW1lKVxuICAgICAgaWYgKGNsYXNzRGVmKSB7XG4gICAgICAgIGNsYXNzRGVmLmV4dGVuZHMgPSBkZWYuZGVmXG4gICAgICB9XG4gICAgfSBlbHNlIGlmIChkZWYua2luZCA9PT0gVHlwZURlZktpbmQuSW1wbCkge1xuICAgICAgLy8gbWVyZ2UgYGltcGxgIGludG8gY2xhc3MgZGVmaW5pdGlvblxuICAgICAgY29uc3QgY2xhc3NEZWYgPSBjbGFzc0RlZnMuZ2V0KGRlZi5uYW1lKVxuICAgICAgaWYgKGNsYXNzRGVmKSB7XG4gICAgICAgIGlmIChjbGFzc0RlZi5kZWYpIHtcbiAgICAgICAgICBjbGFzc0RlZi5kZWYgKz0gJ1xcbidcbiAgICAgICAgfVxuXG4gICAgICAgIGNsYXNzRGVmLmRlZiArPSBkZWYuZGVmXG4gICAgICAgIC8vIENvbnZlcnQgYW55IHJlbWFpbmluZyBcXG4gc2VxdWVuY2VzIGluIHRoZSBtZXJnZWQgZGVmIHRvIGFjdHVhbCBuZXdsaW5lc1xuICAgICAgICBpZiAoY2xhc3NEZWYuZGVmKSB7XG4gICAgICAgICAgY2xhc3NEZWYuZGVmID0gY2xhc3NEZWYuZGVmLnJlcGxhY2UoL1xcXFxuL2csICdcXG4nKVxuICAgICAgICB9XG4gICAgICB9XG4gICAgfSBlbHNlIHtcbiAgICAgIGdyb3VwLnB1c2goZGVmKVxuICAgIH1cbiAgfVxuXG4gIHJldHVybiBuYW1lc3BhY2VHcm91cGVkXG59XG5cbmV4cG9ydCBmdW5jdGlvbiBjb3JyZWN0U3RyaW5nSWRlbnQoc3JjOiBzdHJpbmcsIGlkZW50OiBudW1iZXIpOiBzdHJpbmcge1xuICBsZXQgYnJhY2tldERlcHRoID0gMFxuICBjb25zdCByZXN1bHQgPSBzcmNcbiAgICAuc3BsaXQoJ1xcbicpXG4gICAgLm1hcCgobGluZSkgPT4ge1xuICAgICAgbGluZSA9IGxpbmUudHJpbSgpXG4gICAgICBpZiAobGluZSA9PT0gJycpIHtcbiAgICAgICAgcmV0dXJuICcnXG4gICAgICB9XG5cbiAgICAgIGNvbnN0IGlzSW5NdWx0aWxpbmVDb21tZW50ID0gbGluZS5zdGFydHNXaXRoKCcqJylcbiAgICAgIGNvbnN0IGlzQ2xvc2luZ0JyYWNrZXQgPSBsaW5lLmVuZHNXaXRoKCd9JylcbiAgICAgIGNvbnN0IGlzT3BlbmluZ0JyYWNrZXQgPSBsaW5lLmVuZHNXaXRoKCd7JylcbiAgICAgIGNvbnN0IGlzVHlwZURlY2xhcmF0aW9uID0gbGluZS5lbmRzV2l0aCgnPScpXG4gICAgICBjb25zdCBpc1R5cGVWYXJpYW50ID0gbGluZS5zdGFydHNXaXRoKCd8JylcblxuICAgICAgbGV0IHJpZ2h0SW5kZW50ID0gaWRlbnRcbiAgICAgIGlmICgoaXNPcGVuaW5nQnJhY2tldCB8fCBpc1R5cGVEZWNsYXJhdGlvbikgJiYgIWlzSW5NdWx0aWxpbmVDb21tZW50KSB7XG4gICAgICAgIGJyYWNrZXREZXB0aCArPSAxXG4gICAgICAgIHJpZ2h0SW5kZW50ICs9IChicmFja2V0RGVwdGggLSAxKSAqIDJcbiAgICAgIH0gZWxzZSB7XG4gICAgICAgIGlmIChcbiAgICAgICAgICBpc0Nsb3NpbmdCcmFja2V0ICYmXG4gICAgICAgICAgYnJhY2tldERlcHRoID4gMCAmJlxuICAgICAgICAgICFpc0luTXVsdGlsaW5lQ29tbWVudCAmJlxuICAgICAgICAgICFpc1R5cGVWYXJpYW50XG4gICAgICAgICkge1xuICAgICAgICAgIGJyYWNrZXREZXB0aCAtPSAxXG4gICAgICAgIH1cbiAgICAgICAgcmlnaHRJbmRlbnQgKz0gYnJhY2tldERlcHRoICogMlxuICAgICAgfVxuXG4gICAgICBpZiAoaXNJbk11bHRpbGluZUNvbW1lbnQpIHtcbiAgICAgICAgcmlnaHRJbmRlbnQgKz0gMVxuICAgICAgfVxuXG4gICAgICBjb25zdCBzID0gYCR7JyAnLnJlcGVhdChyaWdodEluZGVudCl9JHtsaW5lfWBcblxuICAgICAgcmV0dXJuIHNcbiAgICB9KVxuICAgIC5qb2luKCdcXG4nKVxuXG4gIHJldHVybiByZXN1bHRcbn1cbiIsImltcG9ydCB7IHJlc29sdmUgfSBmcm9tICdub2RlOnBhdGgnXG5cbmltcG9ydCB7IHJlYWROYXBpQ29uZmlnIH0gZnJvbSAnLi9jb25maWcuanMnXG5cbmludGVyZmFjZSBNaW5pbWFsTmFwaU9wdGlvbnMge1xuICBjd2Q6IHN0cmluZ1xuICBjb25maWdQYXRoPzogc3RyaW5nXG4gIHBhY2thZ2VKc29uUGF0aD86IHN0cmluZ1xufVxuXG5leHBvcnQgYXN5bmMgZnVuY3Rpb24gcmVhZENvbmZpZyhvcHRpb25zOiBNaW5pbWFsTmFwaU9wdGlvbnMpIHtcbiAgY29uc3QgcmVzb2x2ZVBhdGggPSAoLi4ucGF0aHM6IHN0cmluZ1tdKSA9PiByZXNvbHZlKG9wdGlvbnMuY3dkLCAuLi5wYXRocylcbiAgY29uc3QgY29uZmlnID0gYXdhaXQgcmVhZE5hcGlDb25maWcoXG4gICAgcmVzb2x2ZVBhdGgob3B0aW9ucy5wYWNrYWdlSnNvblBhdGggPz8gJ3BhY2thZ2UuanNvbicpLFxuICAgIG9wdGlvbnMuY29uZmlnUGF0aCA/IHJlc29sdmVQYXRoKG9wdGlvbnMuY29uZmlnUGF0aCkgOiB1bmRlZmluZWQsXG4gIClcbiAgcmV0dXJuIGNvbmZpZ1xufVxuIiwiaW1wb3J0IHsgam9pbiwgcmVzb2x2ZSwgcGFyc2UgfSBmcm9tICdub2RlOnBhdGgnXG5cbmltcG9ydCAqIGFzIGNvbG9ycyBmcm9tICdjb2xvcmV0dGUnXG5cbmltcG9ydCB7XG4gIGFwcGx5RGVmYXVsdEFydGlmYWN0c09wdGlvbnMsXG4gIHR5cGUgQXJ0aWZhY3RzT3B0aW9ucyxcbn0gZnJvbSAnLi4vZGVmL2FydGlmYWN0cy5qcydcbmltcG9ydCB7XG4gIHJlYWROYXBpQ29uZmlnLFxuICBkZWJ1Z0ZhY3RvcnksXG4gIHJlYWRGaWxlQXN5bmMsXG4gIHdyaXRlRmlsZUFzeW5jLFxuICBVbmlBcmNoc0J5UGxhdGZvcm0sXG4gIHJlYWRkaXJBc3luYyxcbn0gZnJvbSAnLi4vdXRpbHMvaW5kZXguanMnXG5cbmNvbnN0IGRlYnVnID0gZGVidWdGYWN0b3J5KCdhcnRpZmFjdHMnKVxuXG5leHBvcnQgYXN5bmMgZnVuY3Rpb24gY29sbGVjdEFydGlmYWN0cyh1c2VyT3B0aW9uczogQXJ0aWZhY3RzT3B0aW9ucykge1xuICBjb25zdCBvcHRpb25zID0gYXBwbHlEZWZhdWx0QXJ0aWZhY3RzT3B0aW9ucyh1c2VyT3B0aW9ucylcblxuICBjb25zdCByZXNvbHZlUGF0aCA9ICguLi5wYXRoczogc3RyaW5nW10pID0+IHJlc29sdmUob3B0aW9ucy5jd2QsIC4uLnBhdGhzKVxuICBjb25zdCBwYWNrYWdlSnNvblBhdGggPSByZXNvbHZlUGF0aChvcHRpb25zLnBhY2thZ2VKc29uUGF0aClcbiAgY29uc3QgeyB0YXJnZXRzLCBiaW5hcnlOYW1lLCBwYWNrYWdlTmFtZSB9ID0gYXdhaXQgcmVhZE5hcGlDb25maWcoXG4gICAgcGFja2FnZUpzb25QYXRoLFxuICAgIG9wdGlvbnMuY29uZmlnUGF0aCA/IHJlc29sdmVQYXRoKG9wdGlvbnMuY29uZmlnUGF0aCkgOiB1bmRlZmluZWQsXG4gIClcblxuICBjb25zdCBkaXN0RGlycyA9IHRhcmdldHMubWFwKChwbGF0Zm9ybSkgPT5cbiAgICBqb2luKG9wdGlvbnMuY3dkLCBvcHRpb25zLm5wbURpciwgcGxhdGZvcm0ucGxhdGZvcm1BcmNoQUJJKSxcbiAgKVxuXG4gIGNvbnN0IHVuaXZlcnNhbFNvdXJjZUJpbnMgPSBuZXcgU2V0KFxuICAgIHRhcmdldHNcbiAgICAgIC5maWx0ZXIoKHBsYXRmb3JtKSA9PiBwbGF0Zm9ybS5hcmNoID09PSAndW5pdmVyc2FsJylcbiAgICAgIC5mbGF0TWFwKChwKSA9PlxuICAgICAgICBVbmlBcmNoc0J5UGxhdGZvcm1bcC5wbGF0Zm9ybV0/Lm1hcCgoYSkgPT4gYCR7cC5wbGF0Zm9ybX0tJHthfWApLFxuICAgICAgKVxuICAgICAgLmZpbHRlcihCb29sZWFuKSBhcyBzdHJpbmdbXSxcbiAgKVxuXG4gIGF3YWl0IGNvbGxlY3ROb2RlQmluYXJpZXMoam9pbihvcHRpb25zLmN3ZCwgb3B0aW9ucy5vdXRwdXREaXIpKS50aGVuKFxuICAgIChvdXRwdXQpID0+XG4gICAgICBQcm9taXNlLmFsbChcbiAgICAgICAgb3V0cHV0Lm1hcChhc3luYyAoZmlsZVBhdGgpID0+IHtcbiAgICAgICAgICBkZWJ1Zy5pbmZvKGBSZWFkIFske2NvbG9ycy55ZWxsb3dCcmlnaHQoZmlsZVBhdGgpfV1gKVxuICAgICAgICAgIGNvbnN0IHNvdXJjZUNvbnRlbnQgPSBhd2FpdCByZWFkRmlsZUFzeW5jKGZpbGVQYXRoKVxuICAgICAgICAgIGNvbnN0IHBhcnNlZE5hbWUgPSBwYXJzZShmaWxlUGF0aClcbiAgICAgICAgICBjb25zdCB0ZXJtcyA9IHBhcnNlZE5hbWUubmFtZS5zcGxpdCgnLicpXG4gICAgICAgICAgY29uc3QgcGxhdGZvcm1BcmNoQUJJID0gdGVybXMucG9wKCkhXG4gICAgICAgICAgY29uc3QgX2JpbmFyeU5hbWUgPSB0ZXJtcy5qb2luKCcuJylcblxuICAgICAgICAgIGlmIChfYmluYXJ5TmFtZSAhPT0gYmluYXJ5TmFtZSkge1xuICAgICAgICAgICAgZGVidWcud2FybihcbiAgICAgICAgICAgICAgYFske19iaW5hcnlOYW1lfV0gaXMgbm90IG1hdGNoZWQgd2l0aCBbJHtiaW5hcnlOYW1lfV0sIHNraXBgLFxuICAgICAgICAgICAgKVxuICAgICAgICAgICAgcmV0dXJuXG4gICAgICAgICAgfVxuICAgICAgICAgIGNvbnN0IGRpciA9IGRpc3REaXJzLmZpbmQoKGRpcikgPT4gZGlyLmluY2x1ZGVzKHBsYXRmb3JtQXJjaEFCSSkpXG4gICAgICAgICAgaWYgKCFkaXIgJiYgdW5pdmVyc2FsU291cmNlQmlucy5oYXMocGxhdGZvcm1BcmNoQUJJKSkge1xuICAgICAgICAgICAgZGVidWcud2FybihcbiAgICAgICAgICAgICAgYFske3BsYXRmb3JtQXJjaEFCSX1dIGhhcyBubyBkaXN0IGRpciBidXQgaXQgaXMgc291cmNlIGJpbiBmb3IgdW5pdmVyc2FsIGFyY2gsIHNraXBgLFxuICAgICAgICAgICAgKVxuICAgICAgICAgICAgcmV0dXJuXG4gICAgICAgICAgfVxuICAgICAgICAgIGlmICghZGlyKSB7XG4gICAgICAgICAgICB0aHJvdyBuZXcgRXJyb3IoYE5vIGRpc3QgZGlyIGZvdW5kIGZvciAke2ZpbGVQYXRofWApXG4gICAgICAgICAgfVxuXG4gICAgICAgICAgY29uc3QgZGlzdEZpbGVQYXRoID0gam9pbihkaXIsIHBhcnNlZE5hbWUuYmFzZSlcbiAgICAgICAgICBkZWJ1Zy5pbmZvKFxuICAgICAgICAgICAgYFdyaXRlIGZpbGUgY29udGVudCB0byBbJHtjb2xvcnMueWVsbG93QnJpZ2h0KGRpc3RGaWxlUGF0aCl9XWAsXG4gICAgICAgICAgKVxuICAgICAgICAgIGF3YWl0IHdyaXRlRmlsZUFzeW5jKGRpc3RGaWxlUGF0aCwgc291cmNlQ29udGVudClcbiAgICAgICAgICBjb25zdCBkaXN0RmlsZVBhdGhMb2NhbCA9IGpvaW4oXG4gICAgICAgICAgICBwYXJzZShwYWNrYWdlSnNvblBhdGgpLmRpcixcbiAgICAgICAgICAgIHBhcnNlZE5hbWUuYmFzZSxcbiAgICAgICAgICApXG4gICAgICAgICAgZGVidWcuaW5mbyhcbiAgICAgICAgICAgIGBXcml0ZSBmaWxlIGNvbnRlbnQgdG8gWyR7Y29sb3JzLnllbGxvd0JyaWdodChkaXN0RmlsZVBhdGhMb2NhbCl9XWAsXG4gICAgICAgICAgKVxuICAgICAgICAgIGF3YWl0IHdyaXRlRmlsZUFzeW5jKGRpc3RGaWxlUGF0aExvY2FsLCBzb3VyY2VDb250ZW50KVxuICAgICAgICB9KSxcbiAgICAgICksXG4gIClcblxuICBjb25zdCB3YXNpVGFyZ2V0ID0gdGFyZ2V0cy5maW5kKCh0KSA9PiB0LnBsYXRmb3JtID09PSAnd2FzaScpXG4gIGlmICh3YXNpVGFyZ2V0KSB7XG4gICAgY29uc3Qgd2FzaURpciA9IGpvaW4oXG4gICAgICBvcHRpb25zLmN3ZCxcbiAgICAgIG9wdGlvbnMubnBtRGlyLFxuICAgICAgd2FzaVRhcmdldC5wbGF0Zm9ybUFyY2hBQkksXG4gICAgKVxuICAgIGNvbnN0IGNqc0ZpbGUgPSBqb2luKFxuICAgICAgb3B0aW9ucy5idWlsZE91dHB1dERpciA/PyBvcHRpb25zLmN3ZCxcbiAgICAgIGAke2JpbmFyeU5hbWV9Lndhc2kuY2pzYCxcbiAgICApXG4gICAgY29uc3Qgd29ya2VyRmlsZSA9IGpvaW4oXG4gICAgICBvcHRpb25zLmJ1aWxkT3V0cHV0RGlyID8/IG9wdGlvbnMuY3dkLFxuICAgICAgYHdhc2ktd29ya2VyLm1qc2AsXG4gICAgKVxuICAgIGNvbnN0IGJyb3dzZXJFbnRyeSA9IGpvaW4oXG4gICAgICBvcHRpb25zLmJ1aWxkT3V0cHV0RGlyID8/IG9wdGlvbnMuY3dkLFxuICAgICAgYCR7YmluYXJ5TmFtZX0ud2FzaS1icm93c2VyLmpzYCxcbiAgICApXG4gICAgY29uc3QgYnJvd3NlcldvcmtlckZpbGUgPSBqb2luKFxuICAgICAgb3B0aW9ucy5idWlsZE91dHB1dERpciA/PyBvcHRpb25zLmN3ZCxcbiAgICAgIGB3YXNpLXdvcmtlci1icm93c2VyLm1qc2AsXG4gICAgKVxuICAgIGRlYnVnLmluZm8oXG4gICAgICBgTW92ZSB3YXNpIGJpbmRpbmcgZmlsZSBbJHtjb2xvcnMueWVsbG93QnJpZ2h0KFxuICAgICAgICBjanNGaWxlLFxuICAgICAgKX1dIHRvIFske2NvbG9ycy55ZWxsb3dCcmlnaHQod2FzaURpcil9XWAsXG4gICAgKVxuICAgIGF3YWl0IHdyaXRlRmlsZUFzeW5jKFxuICAgICAgam9pbih3YXNpRGlyLCBgJHtiaW5hcnlOYW1lfS53YXNpLmNqc2ApLFxuICAgICAgYXdhaXQgcmVhZEZpbGVBc3luYyhjanNGaWxlKSxcbiAgICApXG4gICAgZGVidWcuaW5mbyhcbiAgICAgIGBNb3ZlIHdhc2kgd29ya2VyIGZpbGUgWyR7Y29sb3JzLnllbGxvd0JyaWdodChcbiAgICAgICAgd29ya2VyRmlsZSxcbiAgICAgICl9XSB0byBbJHtjb2xvcnMueWVsbG93QnJpZ2h0KHdhc2lEaXIpfV1gLFxuICAgIClcbiAgICBhd2FpdCB3cml0ZUZpbGVBc3luYyhcbiAgICAgIGpvaW4od2FzaURpciwgYHdhc2ktd29ya2VyLm1qc2ApLFxuICAgICAgYXdhaXQgcmVhZEZpbGVBc3luYyh3b3JrZXJGaWxlKSxcbiAgICApXG4gICAgZGVidWcuaW5mbyhcbiAgICAgIGBNb3ZlIHdhc2kgYnJvd3NlciBlbnRyeSBmaWxlIFske2NvbG9ycy55ZWxsb3dCcmlnaHQoXG4gICAgICAgIGJyb3dzZXJFbnRyeSxcbiAgICAgICl9XSB0byBbJHtjb2xvcnMueWVsbG93QnJpZ2h0KHdhc2lEaXIpfV1gLFxuICAgIClcbiAgICBhd2FpdCB3cml0ZUZpbGVBc3luYyhcbiAgICAgIGpvaW4od2FzaURpciwgYCR7YmluYXJ5TmFtZX0ud2FzaS1icm93c2VyLmpzYCksXG4gICAgICAvLyBodHRwczovL2dpdGh1Yi5jb20vdml0ZWpzL3ZpdGUvaXNzdWVzLzg0MjdcbiAgICAgIChhd2FpdCByZWFkRmlsZUFzeW5jKGJyb3dzZXJFbnRyeSwgJ3V0ZjgnKSkucmVwbGFjZShcbiAgICAgICAgYG5ldyBVUkwoJy4vd2FzaS13b3JrZXItYnJvd3Nlci5tanMnLCBpbXBvcnQubWV0YS51cmwpYCxcbiAgICAgICAgYG5ldyBVUkwoJyR7cGFja2FnZU5hbWV9LXdhc20zMi13YXNpL3dhc2ktd29ya2VyLWJyb3dzZXIubWpzJywgaW1wb3J0Lm1ldGEudXJsKWAsXG4gICAgICApLFxuICAgIClcbiAgICBkZWJ1Zy5pbmZvKFxuICAgICAgYE1vdmUgd2FzaSBicm93c2VyIHdvcmtlciBmaWxlIFske2NvbG9ycy55ZWxsb3dCcmlnaHQoXG4gICAgICAgIGJyb3dzZXJXb3JrZXJGaWxlLFxuICAgICAgKX1dIHRvIFske2NvbG9ycy55ZWxsb3dCcmlnaHQod2FzaURpcil9XWAsXG4gICAgKVxuICAgIGF3YWl0IHdyaXRlRmlsZUFzeW5jKFxuICAgICAgam9pbih3YXNpRGlyLCBgd2FzaS13b3JrZXItYnJvd3Nlci5tanNgKSxcbiAgICAgIGF3YWl0IHJlYWRGaWxlQXN5bmMoYnJvd3NlcldvcmtlckZpbGUpLFxuICAgIClcbiAgfVxufVxuXG5hc3luYyBmdW5jdGlvbiBjb2xsZWN0Tm9kZUJpbmFyaWVzKHJvb3Q6IHN0cmluZykge1xuICBjb25zdCBmaWxlcyA9IGF3YWl0IHJlYWRkaXJBc3luYyhyb290LCB7IHdpdGhGaWxlVHlwZXM6IHRydWUgfSlcbiAgY29uc3Qgbm9kZUJpbmFyaWVzID0gZmlsZXNcbiAgICAuZmlsdGVyKFxuICAgICAgKGZpbGUpID0+XG4gICAgICAgIGZpbGUuaXNGaWxlKCkgJiZcbiAgICAgICAgKGZpbGUubmFtZS5lbmRzV2l0aCgnLm5vZGUnKSB8fCBmaWxlLm5hbWUuZW5kc1dpdGgoJy53YXNtJykpLFxuICAgIClcbiAgICAubWFwKChmaWxlKSA9PiBqb2luKHJvb3QsIGZpbGUubmFtZSkpXG5cbiAgY29uc3QgZGlycyA9IGZpbGVzLmZpbHRlcigoZmlsZSkgPT4gZmlsZS5pc0RpcmVjdG9yeSgpKVxuICBmb3IgKGNvbnN0IGRpciBvZiBkaXJzKSB7XG4gICAgaWYgKGRpci5uYW1lICE9PSAnbm9kZV9tb2R1bGVzJykge1xuICAgICAgbm9kZUJpbmFyaWVzLnB1c2goLi4uKGF3YWl0IGNvbGxlY3ROb2RlQmluYXJpZXMoam9pbihyb290LCBkaXIubmFtZSkpKSlcbiAgICB9XG4gIH1cbiAgcmV0dXJuIG5vZGVCaW5hcmllc1xufVxuIiwiZXhwb3J0IGZ1bmN0aW9uIGNyZWF0ZUNqc0JpbmRpbmcoXG4gIGxvY2FsTmFtZTogc3RyaW5nLFxuICBwa2dOYW1lOiBzdHJpbmcsXG4gIGlkZW50czogc3RyaW5nW10sXG4gIHBhY2thZ2VWZXJzaW9uPzogc3RyaW5nLFxuKTogc3RyaW5nIHtcbiAgcmV0dXJuIGAke2JpbmRpbmdIZWFkZXJ9XG4ke2NyZWF0ZUNvbW1vbkJpbmRpbmcobG9jYWxOYW1lLCBwa2dOYW1lLCBwYWNrYWdlVmVyc2lvbil9XG5tb2R1bGUuZXhwb3J0cyA9IG5hdGl2ZUJpbmRpbmdcbiR7aWRlbnRzXG4gIC5tYXAoKGlkZW50KSA9PiBgbW9kdWxlLmV4cG9ydHMuJHtpZGVudH0gPSBuYXRpdmVCaW5kaW5nLiR7aWRlbnR9YClcbiAgLmpvaW4oJ1xcbicpfVxuYFxufVxuXG5leHBvcnQgZnVuY3Rpb24gY3JlYXRlRXNtQmluZGluZyhcbiAgbG9jYWxOYW1lOiBzdHJpbmcsXG4gIHBrZ05hbWU6IHN0cmluZyxcbiAgaWRlbnRzOiBzdHJpbmdbXSxcbiAgcGFja2FnZVZlcnNpb24/OiBzdHJpbmcsXG4pOiBzdHJpbmcge1xuICByZXR1cm4gYCR7YmluZGluZ0hlYWRlcn1cbmltcG9ydCB7IGNyZWF0ZVJlcXVpcmUgfSBmcm9tICdub2RlOm1vZHVsZSdcbmNvbnN0IHJlcXVpcmUgPSBjcmVhdGVSZXF1aXJlKGltcG9ydC5tZXRhLnVybClcbmNvbnN0IF9fZGlybmFtZSA9IG5ldyBVUkwoJy4nLCBpbXBvcnQubWV0YS51cmwpLnBhdGhuYW1lXG5cbiR7Y3JlYXRlQ29tbW9uQmluZGluZyhsb2NhbE5hbWUsIHBrZ05hbWUsIHBhY2thZ2VWZXJzaW9uKX1cbmNvbnN0IHsgJHtpZGVudHMuam9pbignLCAnKX0gfSA9IG5hdGl2ZUJpbmRpbmdcbiR7aWRlbnRzLm1hcCgoaWRlbnQpID0+IGBleHBvcnQgeyAke2lkZW50fSB9YCkuam9pbignXFxuJyl9XG5gXG59XG5cbmNvbnN0IGJpbmRpbmdIZWFkZXIgPSBgLy8gcHJldHRpZXItaWdub3JlXG4vKiBlc2xpbnQtZGlzYWJsZSAqL1xuLy8gQHRzLW5vY2hlY2tcbi8qIGF1dG8tZ2VuZXJhdGVkIGJ5IE5BUEktUlMgKi9cbmBcblxuZnVuY3Rpb24gY3JlYXRlQ29tbW9uQmluZGluZyhcbiAgbG9jYWxOYW1lOiBzdHJpbmcsXG4gIHBrZ05hbWU6IHN0cmluZyxcbiAgcGFja2FnZVZlcnNpb24/OiBzdHJpbmcsXG4pOiBzdHJpbmcge1xuICBmdW5jdGlvbiByZXF1aXJlVHVwbGUodHVwbGU6IHN0cmluZywgaWRlbnRTaXplID0gOCkge1xuICAgIGNvbnN0IGlkZW50TG93ID0gJyAnLnJlcGVhdChpZGVudFNpemUgLSAyKVxuICAgIGNvbnN0IGlkZW50ID0gJyAnLnJlcGVhdChpZGVudFNpemUpXG4gICAgY29uc3QgdmVyc2lvbkNoZWNrID0gcGFja2FnZVZlcnNpb25cbiAgICAgID8gYFxuJHtpZGVudExvd310cnkge1xuJHtpZGVudH1jb25zdCBiaW5kaW5nID0gcmVxdWlyZSgnJHtwa2dOYW1lfS0ke3R1cGxlfScpXG4ke2lkZW50fWNvbnN0IGJpbmRpbmdQYWNrYWdlVmVyc2lvbiA9IHJlcXVpcmUoJyR7cGtnTmFtZX0tJHt0dXBsZX0vcGFja2FnZS5qc29uJykudmVyc2lvblxuJHtpZGVudH1pZiAoYmluZGluZ1BhY2thZ2VWZXJzaW9uICE9PSAnJHtwYWNrYWdlVmVyc2lvbn0nICYmIHByb2Nlc3MuZW52Lk5BUElfUlNfRU5GT1JDRV9WRVJTSU9OX0NIRUNLICYmIHByb2Nlc3MuZW52Lk5BUElfUlNfRU5GT1JDRV9WRVJTSU9OX0NIRUNLICE9PSAnMCcpIHtcbiR7aWRlbnR9ICB0aHJvdyBuZXcgRXJyb3IoXFxgTmF0aXZlIGJpbmRpbmcgcGFja2FnZSB2ZXJzaW9uIG1pc21hdGNoLCBleHBlY3RlZCAke3BhY2thZ2VWZXJzaW9ufSBidXQgZ290IFxcJHtiaW5kaW5nUGFja2FnZVZlcnNpb259LiBZb3UgY2FuIHJlaW5zdGFsbCBkZXBlbmRlbmNpZXMgdG8gZml4IHRoaXMgaXNzdWUuXFxgKVxuJHtpZGVudH19XG4ke2lkZW50fXJldHVybiBiaW5kaW5nXG4ke2lkZW50TG93fX0gY2F0Y2ggKGUpIHtcbiR7aWRlbnR9bG9hZEVycm9ycy5wdXNoKGUpXG4ke2lkZW50TG93fX1gXG4gICAgICA6IGBcbiR7aWRlbnRMb3d9dHJ5IHtcbiR7aWRlbnR9cmV0dXJuIHJlcXVpcmUoJyR7cGtnTmFtZX0tJHt0dXBsZX0nKVxuJHtpZGVudExvd319IGNhdGNoIChlKSB7XG4ke2lkZW50fWxvYWRFcnJvcnMucHVzaChlKVxuJHtpZGVudExvd319YFxuICAgIHJldHVybiBgdHJ5IHtcbiR7aWRlbnR9cmV0dXJuIHJlcXVpcmUoJy4vJHtsb2NhbE5hbWV9LiR7dHVwbGV9Lm5vZGUnKVxuJHtpZGVudExvd319IGNhdGNoIChlKSB7XG4ke2lkZW50fWxvYWRFcnJvcnMucHVzaChlKVxuJHtpZGVudExvd319JHt2ZXJzaW9uQ2hlY2t9YFxuICB9XG5cbiAgcmV0dXJuIGBjb25zdCB7IHJlYWRGaWxlU3luYyB9ID0gcmVxdWlyZSgnbm9kZTpmcycpXG5sZXQgbmF0aXZlQmluZGluZyA9IG51bGxcbmNvbnN0IGxvYWRFcnJvcnMgPSBbXVxuXG5jb25zdCBpc011c2wgPSAoKSA9PiB7XG4gIGxldCBtdXNsID0gZmFsc2VcbiAgaWYgKHByb2Nlc3MucGxhdGZvcm0gPT09ICdsaW51eCcpIHtcbiAgICBtdXNsID0gaXNNdXNsRnJvbUZpbGVzeXN0ZW0oKVxuICAgIGlmIChtdXNsID09PSBudWxsKSB7XG4gICAgICBtdXNsID0gaXNNdXNsRnJvbVJlcG9ydCgpXG4gICAgfVxuICAgIGlmIChtdXNsID09PSBudWxsKSB7XG4gICAgICBtdXNsID0gaXNNdXNsRnJvbUNoaWxkUHJvY2VzcygpXG4gICAgfVxuICB9XG4gIHJldHVybiBtdXNsXG59XG5cbmNvbnN0IGlzRmlsZU11c2wgPSAoZikgPT4gZi5pbmNsdWRlcygnbGliYy5tdXNsLScpIHx8IGYuaW5jbHVkZXMoJ2xkLW11c2wtJylcblxuY29uc3QgaXNNdXNsRnJvbUZpbGVzeXN0ZW0gPSAoKSA9PiB7XG4gIHRyeSB7XG4gICAgcmV0dXJuIHJlYWRGaWxlU3luYygnL3Vzci9iaW4vbGRkJywgJ3V0Zi04JykuaW5jbHVkZXMoJ211c2wnKVxuICB9IGNhdGNoIHtcbiAgICByZXR1cm4gbnVsbFxuICB9XG59XG5cbmNvbnN0IGlzTXVzbEZyb21SZXBvcnQgPSAoKSA9PiB7XG4gIGxldCByZXBvcnQgPSBudWxsXG4gIGlmICh0eXBlb2YgcHJvY2Vzcy5yZXBvcnQ/LmdldFJlcG9ydCA9PT0gJ2Z1bmN0aW9uJykge1xuICAgIHByb2Nlc3MucmVwb3J0LmV4Y2x1ZGVOZXR3b3JrID0gdHJ1ZVxuICAgIHJlcG9ydCA9IHByb2Nlc3MucmVwb3J0LmdldFJlcG9ydCgpXG4gIH1cbiAgaWYgKCFyZXBvcnQpIHtcbiAgICByZXR1cm4gbnVsbFxuICB9XG4gIGlmIChyZXBvcnQuaGVhZGVyICYmIHJlcG9ydC5oZWFkZXIuZ2xpYmNWZXJzaW9uUnVudGltZSkge1xuICAgIHJldHVybiBmYWxzZVxuICB9XG4gIGlmIChBcnJheS5pc0FycmF5KHJlcG9ydC5zaGFyZWRPYmplY3RzKSkge1xuICAgIGlmIChyZXBvcnQuc2hhcmVkT2JqZWN0cy5zb21lKGlzRmlsZU11c2wpKSB7XG4gICAgICByZXR1cm4gdHJ1ZVxuICAgIH1cbiAgfVxuICByZXR1cm4gZmFsc2Vcbn1cblxuY29uc3QgaXNNdXNsRnJvbUNoaWxkUHJvY2VzcyA9ICgpID0+IHtcbiAgdHJ5IHtcbiAgICByZXR1cm4gcmVxdWlyZSgnY2hpbGRfcHJvY2VzcycpLmV4ZWNTeW5jKCdsZGQgLS12ZXJzaW9uJywgeyBlbmNvZGluZzogJ3V0ZjgnIH0pLmluY2x1ZGVzKCdtdXNsJylcbiAgfSBjYXRjaCAoZSkge1xuICAgIC8vIElmIHdlIHJlYWNoIHRoaXMgY2FzZSwgd2UgZG9uJ3Qga25vdyBpZiB0aGUgc3lzdGVtIGlzIG11c2wgb3Igbm90LCBzbyBpcyBiZXR0ZXIgdG8ganVzdCBmYWxsYmFjayB0byBmYWxzZVxuICAgIHJldHVybiBmYWxzZVxuICB9XG59XG5cbmZ1bmN0aW9uIHJlcXVpcmVOYXRpdmUoKSB7XG4gIGlmIChwcm9jZXNzLmVudi5OQVBJX1JTX05BVElWRV9MSUJSQVJZX1BBVEgpIHtcbiAgICB0cnkge1xuICAgICAgcmV0dXJuIHJlcXVpcmUocHJvY2Vzcy5lbnYuTkFQSV9SU19OQVRJVkVfTElCUkFSWV9QQVRIKTtcbiAgICB9IGNhdGNoIChlcnIpIHtcbiAgICAgIGxvYWRFcnJvcnMucHVzaChlcnIpXG4gICAgfVxuICB9IGVsc2UgaWYgKHByb2Nlc3MucGxhdGZvcm0gPT09ICdhbmRyb2lkJykge1xuICAgIGlmIChwcm9jZXNzLmFyY2ggPT09ICdhcm02NCcpIHtcbiAgICAgICR7cmVxdWlyZVR1cGxlKCdhbmRyb2lkLWFybTY0Jyl9XG4gICAgfSBlbHNlIGlmIChwcm9jZXNzLmFyY2ggPT09ICdhcm0nKSB7XG4gICAgICAke3JlcXVpcmVUdXBsZSgnYW5kcm9pZC1hcm0tZWFiaScpfVxuICAgIH0gZWxzZSB7XG4gICAgICBsb2FkRXJyb3JzLnB1c2gobmV3IEVycm9yKFxcYFVuc3VwcG9ydGVkIGFyY2hpdGVjdHVyZSBvbiBBbmRyb2lkIFxcJHtwcm9jZXNzLmFyY2h9XFxgKSlcbiAgICB9XG4gIH0gZWxzZSBpZiAocHJvY2Vzcy5wbGF0Zm9ybSA9PT0gJ3dpbjMyJykge1xuICAgIGlmIChwcm9jZXNzLmFyY2ggPT09ICd4NjQnKSB7XG4gICAgICBpZiAocHJvY2Vzcy5jb25maWc/LnZhcmlhYmxlcz8uc2hsaWJfc3VmZml4ID09PSAnZGxsLmEnIHx8IHByb2Nlc3MuY29uZmlnPy52YXJpYWJsZXM/Lm5vZGVfdGFyZ2V0X3R5cGUgPT09ICdzaGFyZWRfbGlicmFyeScpIHtcbiAgICAgICAgJHtyZXF1aXJlVHVwbGUoJ3dpbjMyLXg2NC1nbnUnKX1cbiAgICAgIH0gZWxzZSB7XG4gICAgICAgICR7cmVxdWlyZVR1cGxlKCd3aW4zMi14NjQtbXN2YycpfVxuICAgICAgfVxuICAgIH0gZWxzZSBpZiAocHJvY2Vzcy5hcmNoID09PSAnaWEzMicpIHtcbiAgICAgICR7cmVxdWlyZVR1cGxlKCd3aW4zMi1pYTMyLW1zdmMnKX1cbiAgICB9IGVsc2UgaWYgKHByb2Nlc3MuYXJjaCA9PT0gJ2FybTY0Jykge1xuICAgICAgJHtyZXF1aXJlVHVwbGUoJ3dpbjMyLWFybTY0LW1zdmMnKX1cbiAgICB9IGVsc2Uge1xuICAgICAgbG9hZEVycm9ycy5wdXNoKG5ldyBFcnJvcihcXGBVbnN1cHBvcnRlZCBhcmNoaXRlY3R1cmUgb24gV2luZG93czogXFwke3Byb2Nlc3MuYXJjaH1cXGApKVxuICAgIH1cbiAgfSBlbHNlIGlmIChwcm9jZXNzLnBsYXRmb3JtID09PSAnZGFyd2luJykge1xuICAgICR7cmVxdWlyZVR1cGxlKCdkYXJ3aW4tdW5pdmVyc2FsJywgNil9XG4gICAgaWYgKHByb2Nlc3MuYXJjaCA9PT0gJ3g2NCcpIHtcbiAgICAgICR7cmVxdWlyZVR1cGxlKCdkYXJ3aW4teDY0Jyl9XG4gICAgfSBlbHNlIGlmIChwcm9jZXNzLmFyY2ggPT09ICdhcm02NCcpIHtcbiAgICAgICR7cmVxdWlyZVR1cGxlKCdkYXJ3aW4tYXJtNjQnKX1cbiAgICB9IGVsc2Uge1xuICAgICAgbG9hZEVycm9ycy5wdXNoKG5ldyBFcnJvcihcXGBVbnN1cHBvcnRlZCBhcmNoaXRlY3R1cmUgb24gbWFjT1M6IFxcJHtwcm9jZXNzLmFyY2h9XFxgKSlcbiAgICB9XG4gIH0gZWxzZSBpZiAocHJvY2Vzcy5wbGF0Zm9ybSA9PT0gJ2ZyZWVic2QnKSB7XG4gICAgaWYgKHByb2Nlc3MuYXJjaCA9PT0gJ3g2NCcpIHtcbiAgICAgICR7cmVxdWlyZVR1cGxlKCdmcmVlYnNkLXg2NCcpfVxuICAgIH0gZWxzZSBpZiAocHJvY2Vzcy5hcmNoID09PSAnYXJtNjQnKSB7XG4gICAgICAke3JlcXVpcmVUdXBsZSgnZnJlZWJzZC1hcm02NCcpfVxuICAgIH0gZWxzZSB7XG4gICAgICBsb2FkRXJyb3JzLnB1c2gobmV3IEVycm9yKFxcYFVuc3VwcG9ydGVkIGFyY2hpdGVjdHVyZSBvbiBGcmVlQlNEOiBcXCR7cHJvY2Vzcy5hcmNofVxcYCkpXG4gICAgfVxuICB9IGVsc2UgaWYgKHByb2Nlc3MucGxhdGZvcm0gPT09ICdsaW51eCcpIHtcbiAgICBpZiAocHJvY2Vzcy5hcmNoID09PSAneDY0Jykge1xuICAgICAgaWYgKGlzTXVzbCgpKSB7XG4gICAgICAgICR7cmVxdWlyZVR1cGxlKCdsaW51eC14NjQtbXVzbCcsIDEwKX1cbiAgICAgIH0gZWxzZSB7XG4gICAgICAgICR7cmVxdWlyZVR1cGxlKCdsaW51eC14NjQtZ251JywgMTApfVxuICAgICAgfVxuICAgIH0gZWxzZSBpZiAocHJvY2Vzcy5hcmNoID09PSAnYXJtNjQnKSB7XG4gICAgICBpZiAoaXNNdXNsKCkpIHtcbiAgICAgICAgJHtyZXF1aXJlVHVwbGUoJ2xpbnV4LWFybTY0LW11c2wnLCAxMCl9XG4gICAgICB9IGVsc2Uge1xuICAgICAgICAke3JlcXVpcmVUdXBsZSgnbGludXgtYXJtNjQtZ251JywgMTApfVxuICAgICAgfVxuICAgIH0gZWxzZSBpZiAocHJvY2Vzcy5hcmNoID09PSAnYXJtJykge1xuICAgICAgaWYgKGlzTXVzbCgpKSB7XG4gICAgICAgICR7cmVxdWlyZVR1cGxlKCdsaW51eC1hcm0tbXVzbGVhYmloZicsIDEwKX1cbiAgICAgIH0gZWxzZSB7XG4gICAgICAgICR7cmVxdWlyZVR1cGxlKCdsaW51eC1hcm0tZ251ZWFiaWhmJywgMTApfVxuICAgICAgfVxuICAgIH0gZWxzZSBpZiAocHJvY2Vzcy5hcmNoID09PSAnbG9vbmc2NCcpIHtcbiAgICAgIGlmIChpc011c2woKSkge1xuICAgICAgICAke3JlcXVpcmVUdXBsZSgnbGludXgtbG9vbmc2NC1tdXNsJywgMTApfVxuICAgICAgfSBlbHNlIHtcbiAgICAgICAgJHtyZXF1aXJlVHVwbGUoJ2xpbnV4LWxvb25nNjQtZ251JywgMTApfVxuICAgICAgfVxuICAgIH0gZWxzZSBpZiAocHJvY2Vzcy5hcmNoID09PSAncmlzY3Y2NCcpIHtcbiAgICAgIGlmIChpc011c2woKSkge1xuICAgICAgICAke3JlcXVpcmVUdXBsZSgnbGludXgtcmlzY3Y2NC1tdXNsJywgMTApfVxuICAgICAgfSBlbHNlIHtcbiAgICAgICAgJHtyZXF1aXJlVHVwbGUoJ2xpbnV4LXJpc2N2NjQtZ251JywgMTApfVxuICAgICAgfVxuICAgIH0gZWxzZSBpZiAocHJvY2Vzcy5hcmNoID09PSAncHBjNjQnKSB7XG4gICAgICAke3JlcXVpcmVUdXBsZSgnbGludXgtcHBjNjQtZ251Jyl9XG4gICAgfSBlbHNlIGlmIChwcm9jZXNzLmFyY2ggPT09ICdzMzkweCcpIHtcbiAgICAgICR7cmVxdWlyZVR1cGxlKCdsaW51eC1zMzkweC1nbnUnKX1cbiAgICB9IGVsc2Uge1xuICAgICAgbG9hZEVycm9ycy5wdXNoKG5ldyBFcnJvcihcXGBVbnN1cHBvcnRlZCBhcmNoaXRlY3R1cmUgb24gTGludXg6IFxcJHtwcm9jZXNzLmFyY2h9XFxgKSlcbiAgICB9XG4gIH0gZWxzZSBpZiAocHJvY2Vzcy5wbGF0Zm9ybSA9PT0gJ29wZW5oYXJtb255Jykge1xuICAgIGlmIChwcm9jZXNzLmFyY2ggPT09ICdhcm02NCcpIHtcbiAgICAgICR7cmVxdWlyZVR1cGxlKCdvcGVuaGFybW9ueS1hcm02NCcpfVxuICAgIH0gZWxzZSBpZiAocHJvY2Vzcy5hcmNoID09PSAneDY0Jykge1xuICAgICAgJHtyZXF1aXJlVHVwbGUoJ29wZW5oYXJtb255LXg2NCcpfVxuICAgIH0gZWxzZSBpZiAocHJvY2Vzcy5hcmNoID09PSAnYXJtJykge1xuICAgICAgJHtyZXF1aXJlVHVwbGUoJ29wZW5oYXJtb255LWFybScpfVxuICAgIH0gZWxzZSB7XG4gICAgICBsb2FkRXJyb3JzLnB1c2gobmV3IEVycm9yKFxcYFVuc3VwcG9ydGVkIGFyY2hpdGVjdHVyZSBvbiBPcGVuSGFybW9ueTogXFwke3Byb2Nlc3MuYXJjaH1cXGApKVxuICAgIH1cbiAgfSBlbHNlIHtcbiAgICBsb2FkRXJyb3JzLnB1c2gobmV3IEVycm9yKFxcYFVuc3VwcG9ydGVkIE9TOiBcXCR7cHJvY2Vzcy5wbGF0Zm9ybX0sIGFyY2hpdGVjdHVyZTogXFwke3Byb2Nlc3MuYXJjaH1cXGApKVxuICB9XG59XG5cbm5hdGl2ZUJpbmRpbmcgPSByZXF1aXJlTmF0aXZlKClcblxuaWYgKCFuYXRpdmVCaW5kaW5nIHx8IHByb2Nlc3MuZW52Lk5BUElfUlNfRk9SQ0VfV0FTSSkge1xuICBsZXQgd2FzaUJpbmRpbmcgPSBudWxsXG4gIGxldCB3YXNpQmluZGluZ0Vycm9yID0gbnVsbFxuICB0cnkge1xuICAgIHdhc2lCaW5kaW5nID0gcmVxdWlyZSgnLi8ke2xvY2FsTmFtZX0ud2FzaS5janMnKVxuICAgIG5hdGl2ZUJpbmRpbmcgPSB3YXNpQmluZGluZ1xuICB9IGNhdGNoIChlcnIpIHtcbiAgICBpZiAocHJvY2Vzcy5lbnYuTkFQSV9SU19GT1JDRV9XQVNJKSB7XG4gICAgICB3YXNpQmluZGluZ0Vycm9yID0gZXJyXG4gICAgfVxuICB9XG4gIGlmICghbmF0aXZlQmluZGluZyB8fCBwcm9jZXNzLmVudi5OQVBJX1JTX0ZPUkNFX1dBU0kpIHtcbiAgICB0cnkge1xuICAgICAgd2FzaUJpbmRpbmcgPSByZXF1aXJlKCcke3BrZ05hbWV9LXdhc20zMi13YXNpJylcbiAgICAgIG5hdGl2ZUJpbmRpbmcgPSB3YXNpQmluZGluZ1xuICAgIH0gY2F0Y2ggKGVycikge1xuICAgICAgaWYgKHByb2Nlc3MuZW52Lk5BUElfUlNfRk9SQ0VfV0FTSSkge1xuICAgICAgICBpZiAoIXdhc2lCaW5kaW5nRXJyb3IpIHtcbiAgICAgICAgICB3YXNpQmluZGluZ0Vycm9yID0gZXJyXG4gICAgICAgIH0gZWxzZSB7XG4gICAgICAgICAgd2FzaUJpbmRpbmdFcnJvci5jYXVzZSA9IGVyclxuICAgICAgICB9XG4gICAgICAgIGxvYWRFcnJvcnMucHVzaChlcnIpXG4gICAgICB9XG4gICAgfVxuICB9XG4gIGlmIChwcm9jZXNzLmVudi5OQVBJX1JTX0ZPUkNFX1dBU0kgPT09ICdlcnJvcicgJiYgIXdhc2lCaW5kaW5nKSB7XG4gICAgY29uc3QgZXJyb3IgPSBuZXcgRXJyb3IoJ1dBU0kgYmluZGluZyBub3QgZm91bmQgYW5kIE5BUElfUlNfRk9SQ0VfV0FTSSBpcyBzZXQgdG8gZXJyb3InKVxuICAgIGVycm9yLmNhdXNlID0gd2FzaUJpbmRpbmdFcnJvclxuICAgIHRocm93IGVycm9yXG4gIH1cbn1cblxuaWYgKCFuYXRpdmVCaW5kaW5nKSB7XG4gIGlmIChsb2FkRXJyb3JzLmxlbmd0aCA+IDApIHtcbiAgICB0aHJvdyBuZXcgRXJyb3IoXG4gICAgICBcXGBDYW5ub3QgZmluZCBuYXRpdmUgYmluZGluZy4gXFxgICtcbiAgICAgICAgXFxgbnBtIGhhcyBhIGJ1ZyByZWxhdGVkIHRvIG9wdGlvbmFsIGRlcGVuZGVuY2llcyAoaHR0cHM6Ly9naXRodWIuY29tL25wbS9jbGkvaXNzdWVzLzQ4MjgpLiBcXGAgK1xuICAgICAgICAnUGxlYXNlIHRyeSBcXGBucG0gaVxcYCBhZ2FpbiBhZnRlciByZW1vdmluZyBib3RoIHBhY2thZ2UtbG9jay5qc29uIGFuZCBub2RlX21vZHVsZXMgZGlyZWN0b3J5LicsXG4gICAgICB7XG4gICAgICAgIGNhdXNlOiBsb2FkRXJyb3JzLnJlZHVjZSgoZXJyLCBjdXIpID0+IHtcbiAgICAgICAgICBjdXIuY2F1c2UgPSBlcnJcbiAgICAgICAgICByZXR1cm4gY3VyXG4gICAgICAgIH0pLFxuICAgICAgfSxcbiAgICApXG4gIH1cbiAgdGhyb3cgbmV3IEVycm9yKFxcYEZhaWxlZCB0byBsb2FkIG5hdGl2ZSBiaW5kaW5nXFxgKVxufVxuYFxufVxuIiwiZXhwb3J0IGNvbnN0IGNyZWF0ZVdhc2lCcm93c2VyQmluZGluZyA9IChcbiAgd2FzaUZpbGVuYW1lOiBzdHJpbmcsXG4gIGluaXRpYWxNZW1vcnkgPSA0MDAwLFxuICBtYXhpbXVtTWVtb3J5ID0gNjU1MzYsXG4gIGZzID0gZmFsc2UsXG4gIGFzeW5jSW5pdCA9IGZhbHNlLFxuICBidWZmZXIgPSBmYWxzZSxcbikgPT4ge1xuICBjb25zdCBmc0ltcG9ydCA9IGZzXG4gICAgPyBidWZmZXJcbiAgICAgID8gYGltcG9ydCB7IG1lbWZzLCBCdWZmZXIgfSBmcm9tICdAbmFwaS1ycy93YXNtLXJ1bnRpbWUvZnMnYFxuICAgICAgOiBgaW1wb3J0IHsgbWVtZnMgfSBmcm9tICdAbmFwaS1ycy93YXNtLXJ1bnRpbWUvZnMnYFxuICAgIDogJydcbiAgY29uc3QgYnVmZmVySW1wb3J0ID0gYnVmZmVyICYmICFmcyA/IGBpbXBvcnQgeyBCdWZmZXIgfSBmcm9tICdidWZmZXInYCA6ICcnXG4gIGNvbnN0IHdhc2lDcmVhdGlvbiA9IGZzXG4gICAgPyBgXG5leHBvcnQgY29uc3QgeyBmczogX19mcywgdm9sOiBfX3ZvbHVtZSB9ID0gbWVtZnMoKVxuXG5jb25zdCBfX3dhc2kgPSBuZXcgX19XQVNJKHtcbiAgdmVyc2lvbjogJ3ByZXZpZXcxJyxcbiAgZnM6IF9fZnMsXG4gIHByZW9wZW5zOiB7XG4gICAgJy8nOiAnLycsXG4gIH0sXG59KWBcbiAgICA6IGBcbmNvbnN0IF9fd2FzaSA9IG5ldyBfX1dBU0koe1xuICB2ZXJzaW9uOiAncHJldmlldzEnLFxufSlgXG5cbiAgY29uc3Qgd29ya2VyRnNIYW5kbGVyID0gZnNcbiAgICA/IGAgICAgd29ya2VyLmFkZEV2ZW50TGlzdGVuZXIoJ21lc3NhZ2UnLCBfX3dhc21DcmVhdGVPbk1lc3NhZ2VGb3JGc1Byb3h5KF9fZnMpKVxcbmBcbiAgICA6ICcnXG5cbiAgY29uc3QgZW1uYXBpSW5qZWN0QnVmZmVyID0gYnVmZmVyXG4gICAgPyAnX19lbW5hcGlDb250ZXh0LmZlYXR1cmUuQnVmZmVyID0gQnVmZmVyJ1xuICAgIDogJydcbiAgY29uc3QgZW1uYXBpSW5zdGFudGlhdGVJbXBvcnQgPSBhc3luY0luaXRcbiAgICA/IGBpbnN0YW50aWF0ZU5hcGlNb2R1bGUgYXMgX19lbW5hcGlJbnN0YW50aWF0ZU5hcGlNb2R1bGVgXG4gICAgOiBgaW5zdGFudGlhdGVOYXBpTW9kdWxlU3luYyBhcyBfX2VtbmFwaUluc3RhbnRpYXRlTmFwaU1vZHVsZVN5bmNgXG4gIGNvbnN0IGVtbmFwaUluc3RhbnRpYXRlQ2FsbCA9IGFzeW5jSW5pdFxuICAgID8gYGF3YWl0IF9fZW1uYXBpSW5zdGFudGlhdGVOYXBpTW9kdWxlYFxuICAgIDogYF9fZW1uYXBpSW5zdGFudGlhdGVOYXBpTW9kdWxlU3luY2BcblxuICByZXR1cm4gYGltcG9ydCB7XG4gIGNyZWF0ZU9uTWVzc2FnZSBhcyBfX3dhc21DcmVhdGVPbk1lc3NhZ2VGb3JGc1Byb3h5LFxuICBnZXREZWZhdWx0Q29udGV4dCBhcyBfX2VtbmFwaUdldERlZmF1bHRDb250ZXh0LFxuICAke2VtbmFwaUluc3RhbnRpYXRlSW1wb3J0fSxcbiAgV0FTSSBhcyBfX1dBU0ksXG59IGZyb20gJ0BuYXBpLXJzL3dhc20tcnVudGltZSdcbiR7ZnNJbXBvcnR9XG4ke2J1ZmZlckltcG9ydH1cbiR7d2FzaUNyZWF0aW9ufVxuXG5jb25zdCBfX3dhc21VcmwgPSBuZXcgVVJMKCcuLyR7d2FzaUZpbGVuYW1lfS53YXNtJywgaW1wb3J0Lm1ldGEudXJsKS5ocmVmXG5jb25zdCBfX2VtbmFwaUNvbnRleHQgPSBfX2VtbmFwaUdldERlZmF1bHRDb250ZXh0KClcbiR7ZW1uYXBpSW5qZWN0QnVmZmVyfVxuXG5jb25zdCBfX3NoYXJlZE1lbW9yeSA9IG5ldyBXZWJBc3NlbWJseS5NZW1vcnkoe1xuICBpbml0aWFsOiAke2luaXRpYWxNZW1vcnl9LFxuICBtYXhpbXVtOiAke21heGltdW1NZW1vcnl9LFxuICBzaGFyZWQ6IHRydWUsXG59KVxuXG5jb25zdCBfX3dhc21GaWxlID0gYXdhaXQgZmV0Y2goX193YXNtVXJsKS50aGVuKChyZXMpID0+IHJlcy5hcnJheUJ1ZmZlcigpKVxuXG5jb25zdCB7XG4gIGluc3RhbmNlOiBfX25hcGlJbnN0YW5jZSxcbiAgbW9kdWxlOiBfX3dhc2lNb2R1bGUsXG4gIG5hcGlNb2R1bGU6IF9fbmFwaU1vZHVsZSxcbn0gPSAke2VtbmFwaUluc3RhbnRpYXRlQ2FsbH0oX193YXNtRmlsZSwge1xuICBjb250ZXh0OiBfX2VtbmFwaUNvbnRleHQsXG4gIGFzeW5jV29ya1Bvb2xTaXplOiA0LFxuICB3YXNpOiBfX3dhc2ksXG4gIG9uQ3JlYXRlV29ya2VyKCkge1xuICAgIGNvbnN0IHdvcmtlciA9IG5ldyBXb3JrZXIobmV3IFVSTCgnLi93YXNpLXdvcmtlci1icm93c2VyLm1qcycsIGltcG9ydC5tZXRhLnVybCksIHtcbiAgICAgIHR5cGU6ICdtb2R1bGUnLFxuICAgIH0pXG4ke3dvcmtlckZzSGFuZGxlcn1cbiAgICByZXR1cm4gd29ya2VyXG4gIH0sXG4gIG92ZXJ3cml0ZUltcG9ydHMoaW1wb3J0T2JqZWN0KSB7XG4gICAgaW1wb3J0T2JqZWN0LmVudiA9IHtcbiAgICAgIC4uLmltcG9ydE9iamVjdC5lbnYsXG4gICAgICAuLi5pbXBvcnRPYmplY3QubmFwaSxcbiAgICAgIC4uLmltcG9ydE9iamVjdC5lbW5hcGksXG4gICAgICBtZW1vcnk6IF9fc2hhcmVkTWVtb3J5LFxuICAgIH1cbiAgICByZXR1cm4gaW1wb3J0T2JqZWN0XG4gIH0sXG4gIGJlZm9yZUluaXQoeyBpbnN0YW5jZSB9KSB7XG4gICAgZm9yIChjb25zdCBuYW1lIG9mIE9iamVjdC5rZXlzKGluc3RhbmNlLmV4cG9ydHMpKSB7XG4gICAgICBpZiAobmFtZS5zdGFydHNXaXRoKCdfX25hcGlfcmVnaXN0ZXJfXycpKSB7XG4gICAgICAgIGluc3RhbmNlLmV4cG9ydHNbbmFtZV0oKVxuICAgICAgfVxuICAgIH1cbiAgfSxcbn0pXG5gXG59XG5cbmV4cG9ydCBjb25zdCBjcmVhdGVXYXNpQmluZGluZyA9IChcbiAgd2FzbUZpbGVOYW1lOiBzdHJpbmcsXG4gIHBhY2thZ2VOYW1lOiBzdHJpbmcsXG4gIGluaXRpYWxNZW1vcnkgPSA0MDAwLFxuICBtYXhpbXVtTWVtb3J5ID0gNjU1MzYsXG4pID0+IGAvKiBlc2xpbnQtZGlzYWJsZSAqL1xuLyogcHJldHRpZXItaWdub3JlICovXG5cbi8qIGF1dG8tZ2VuZXJhdGVkIGJ5IE5BUEktUlMgKi9cblxuY29uc3QgX19ub2RlRnMgPSByZXF1aXJlKCdub2RlOmZzJylcbmNvbnN0IF9fbm9kZVBhdGggPSByZXF1aXJlKCdub2RlOnBhdGgnKVxuY29uc3QgeyBXQVNJOiBfX25vZGVXQVNJIH0gPSByZXF1aXJlKCdub2RlOndhc2knKVxuY29uc3QgeyBXb3JrZXIgfSA9IHJlcXVpcmUoJ25vZGU6d29ya2VyX3RocmVhZHMnKVxuXG5jb25zdCB7XG4gIGNyZWF0ZU9uTWVzc2FnZTogX193YXNtQ3JlYXRlT25NZXNzYWdlRm9yRnNQcm94eSxcbiAgZ2V0RGVmYXVsdENvbnRleHQ6IF9fZW1uYXBpR2V0RGVmYXVsdENvbnRleHQsXG4gIGluc3RhbnRpYXRlTmFwaU1vZHVsZVN5bmM6IF9fZW1uYXBpSW5zdGFudGlhdGVOYXBpTW9kdWxlU3luYyxcbn0gPSByZXF1aXJlKCdAbmFwaS1ycy93YXNtLXJ1bnRpbWUnKVxuXG5jb25zdCBfX3Jvb3REaXIgPSBfX25vZGVQYXRoLnBhcnNlKHByb2Nlc3MuY3dkKCkpLnJvb3RcblxuY29uc3QgX193YXNpID0gbmV3IF9fbm9kZVdBU0koe1xuICB2ZXJzaW9uOiAncHJldmlldzEnLFxuICBlbnY6IHByb2Nlc3MuZW52LFxuICBwcmVvcGVuczoge1xuICAgIFtfX3Jvb3REaXJdOiBfX3Jvb3REaXIsXG4gIH1cbn0pXG5cbmNvbnN0IF9fZW1uYXBpQ29udGV4dCA9IF9fZW1uYXBpR2V0RGVmYXVsdENvbnRleHQoKVxuXG5jb25zdCBfX3NoYXJlZE1lbW9yeSA9IG5ldyBXZWJBc3NlbWJseS5NZW1vcnkoe1xuICBpbml0aWFsOiAke2luaXRpYWxNZW1vcnl9LFxuICBtYXhpbXVtOiAke21heGltdW1NZW1vcnl9LFxuICBzaGFyZWQ6IHRydWUsXG59KVxuXG5sZXQgX193YXNtRmlsZVBhdGggPSBfX25vZGVQYXRoLmpvaW4oX19kaXJuYW1lLCAnJHt3YXNtRmlsZU5hbWV9Lndhc20nKVxuY29uc3QgX193YXNtRGVidWdGaWxlUGF0aCA9IF9fbm9kZVBhdGguam9pbihfX2Rpcm5hbWUsICcke3dhc21GaWxlTmFtZX0uZGVidWcud2FzbScpXG5cbmlmIChfX25vZGVGcy5leGlzdHNTeW5jKF9fd2FzbURlYnVnRmlsZVBhdGgpKSB7XG4gIF9fd2FzbUZpbGVQYXRoID0gX193YXNtRGVidWdGaWxlUGF0aFxufSBlbHNlIGlmICghX19ub2RlRnMuZXhpc3RzU3luYyhfX3dhc21GaWxlUGF0aCkpIHtcbiAgdHJ5IHtcbiAgICBfX3dhc21GaWxlUGF0aCA9IHJlcXVpcmUucmVzb2x2ZSgnJHtwYWNrYWdlTmFtZX0td2FzbTMyLXdhc2kvJHt3YXNtRmlsZU5hbWV9Lndhc20nKVxuICB9IGNhdGNoIHtcbiAgICB0aHJvdyBuZXcgRXJyb3IoJ0Nhbm5vdCBmaW5kICR7d2FzbUZpbGVOYW1lfS53YXNtIGZpbGUsIGFuZCAke3BhY2thZ2VOYW1lfS13YXNtMzItd2FzaSBwYWNrYWdlIGlzIG5vdCBpbnN0YWxsZWQuJylcbiAgfVxufVxuXG5jb25zdCB7IGluc3RhbmNlOiBfX25hcGlJbnN0YW5jZSwgbW9kdWxlOiBfX3dhc2lNb2R1bGUsIG5hcGlNb2R1bGU6IF9fbmFwaU1vZHVsZSB9ID0gX19lbW5hcGlJbnN0YW50aWF0ZU5hcGlNb2R1bGVTeW5jKF9fbm9kZUZzLnJlYWRGaWxlU3luYyhfX3dhc21GaWxlUGF0aCksIHtcbiAgY29udGV4dDogX19lbW5hcGlDb250ZXh0LFxuICBhc3luY1dvcmtQb29sU2l6ZTogKGZ1bmN0aW9uKCkge1xuICAgIGNvbnN0IHRocmVhZHNTaXplRnJvbUVudiA9IE51bWJlcihwcm9jZXNzLmVudi5OQVBJX1JTX0FTWU5DX1dPUktfUE9PTF9TSVpFID8/IHByb2Nlc3MuZW52LlVWX1RIUkVBRFBPT0xfU0laRSlcbiAgICAvLyBOYU4gPiAwIGlzIGZhbHNlXG4gICAgaWYgKHRocmVhZHNTaXplRnJvbUVudiA+IDApIHtcbiAgICAgIHJldHVybiB0aHJlYWRzU2l6ZUZyb21FbnZcbiAgICB9IGVsc2Uge1xuICAgICAgcmV0dXJuIDRcbiAgICB9XG4gIH0pKCksXG4gIHJldXNlV29ya2VyOiB0cnVlLFxuICB3YXNpOiBfX3dhc2ksXG4gIG9uQ3JlYXRlV29ya2VyKCkge1xuICAgIGNvbnN0IHdvcmtlciA9IG5ldyBXb3JrZXIoX19ub2RlUGF0aC5qb2luKF9fZGlybmFtZSwgJ3dhc2ktd29ya2VyLm1qcycpLCB7XG4gICAgICBlbnY6IHByb2Nlc3MuZW52LFxuICAgIH0pXG4gICAgd29ya2VyLm9ubWVzc2FnZSA9ICh7IGRhdGEgfSkgPT4ge1xuICAgICAgX193YXNtQ3JlYXRlT25NZXNzYWdlRm9yRnNQcm94eShfX25vZGVGcykoZGF0YSlcbiAgICB9XG5cbiAgICAvLyBUaGUgbWFpbiB0aHJlYWQgb2YgTm9kZS5qcyB3YWl0cyBmb3IgYWxsIHRoZSBhY3RpdmUgaGFuZGxlcyBiZWZvcmUgZXhpdGluZy5cbiAgICAvLyBCdXQgUnVzdCB0aHJlYWRzIGFyZSBuZXZlciB3YWl0ZWQgd2l0aG91dCBcXGB0aHJlYWQ6OmpvaW5cXGAuXG4gICAgLy8gU28gaGVyZSB3ZSBoYWNrIHRoZSBjb2RlIG9mIE5vZGUuanMgdG8gcHJldmVudCB0aGUgd29ya2VycyBmcm9tIGJlaW5nIHJlZmVyZW5jZWQgKGFjdGl2ZSkuXG4gICAgLy8gQWNjb3JkaW5nIHRvIGh0dHBzOi8vZ2l0aHViLmNvbS9ub2RlanMvbm9kZS9ibG9iLzE5ZTBkNDcyNzI4Yzc5ZDQxOGI3NGJkZGZmNTg4YmVhNzBhNDAzZDAvbGliL2ludGVybmFsL3dvcmtlci5qcyNMNDE1LFxuICAgIC8vIGEgd29ya2VyIGlzIGNvbnNpc3Qgb2YgdHdvIGhhbmRsZXM6IGtQdWJsaWNQb3J0IGFuZCBrSGFuZGxlLlxuICAgIHtcbiAgICAgIGNvbnN0IGtQdWJsaWNQb3J0ID0gT2JqZWN0LmdldE93blByb3BlcnR5U3ltYm9scyh3b3JrZXIpLmZpbmQocyA9PlxuICAgICAgICBzLnRvU3RyaW5nKCkuaW5jbHVkZXMoXCJrUHVibGljUG9ydFwiKVxuICAgICAgKTtcbiAgICAgIGlmIChrUHVibGljUG9ydCkge1xuICAgICAgICB3b3JrZXJba1B1YmxpY1BvcnRdLnJlZiA9ICgpID0+IHt9O1xuICAgICAgfVxuXG4gICAgICBjb25zdCBrSGFuZGxlID0gT2JqZWN0LmdldE93blByb3BlcnR5U3ltYm9scyh3b3JrZXIpLmZpbmQocyA9PlxuICAgICAgICBzLnRvU3RyaW5nKCkuaW5jbHVkZXMoXCJrSGFuZGxlXCIpXG4gICAgICApO1xuICAgICAgaWYgKGtIYW5kbGUpIHtcbiAgICAgICAgd29ya2VyW2tIYW5kbGVdLnJlZiA9ICgpID0+IHt9O1xuICAgICAgfVxuXG4gICAgICB3b3JrZXIudW5yZWYoKTtcbiAgICB9XG4gICAgcmV0dXJuIHdvcmtlclxuICB9LFxuICBvdmVyd3JpdGVJbXBvcnRzKGltcG9ydE9iamVjdCkge1xuICAgIGltcG9ydE9iamVjdC5lbnYgPSB7XG4gICAgICAuLi5pbXBvcnRPYmplY3QuZW52LFxuICAgICAgLi4uaW1wb3J0T2JqZWN0Lm5hcGksXG4gICAgICAuLi5pbXBvcnRPYmplY3QuZW1uYXBpLFxuICAgICAgbWVtb3J5OiBfX3NoYXJlZE1lbW9yeSxcbiAgICB9XG4gICAgcmV0dXJuIGltcG9ydE9iamVjdFxuICB9LFxuICBiZWZvcmVJbml0KHsgaW5zdGFuY2UgfSkge1xuICAgIGZvciAoY29uc3QgbmFtZSBvZiBPYmplY3Qua2V5cyhpbnN0YW5jZS5leHBvcnRzKSkge1xuICAgICAgaWYgKG5hbWUuc3RhcnRzV2l0aCgnX19uYXBpX3JlZ2lzdGVyX18nKSkge1xuICAgICAgICBpbnN0YW5jZS5leHBvcnRzW25hbWVdKClcbiAgICAgIH1cbiAgICB9XG4gIH0sXG59KVxuYFxuIiwiZXhwb3J0IGNvbnN0IFdBU0lfV09SS0VSX1RFTVBMQVRFID0gYGltcG9ydCBmcyBmcm9tIFwibm9kZTpmc1wiO1xuaW1wb3J0IHsgY3JlYXRlUmVxdWlyZSB9IGZyb20gXCJub2RlOm1vZHVsZVwiO1xuaW1wb3J0IHsgcGFyc2UgfSBmcm9tIFwibm9kZTpwYXRoXCI7XG5pbXBvcnQgeyBXQVNJIH0gZnJvbSBcIm5vZGU6d2FzaVwiO1xuaW1wb3J0IHsgcGFyZW50UG9ydCwgV29ya2VyIH0gZnJvbSBcIm5vZGU6d29ya2VyX3RocmVhZHNcIjtcblxuY29uc3QgcmVxdWlyZSA9IGNyZWF0ZVJlcXVpcmUoaW1wb3J0Lm1ldGEudXJsKTtcblxuY29uc3QgeyBpbnN0YW50aWF0ZU5hcGlNb2R1bGVTeW5jLCBNZXNzYWdlSGFuZGxlciwgZ2V0RGVmYXVsdENvbnRleHQgfSA9IHJlcXVpcmUoXCJAbmFwaS1ycy93YXNtLXJ1bnRpbWVcIik7XG5cbmlmIChwYXJlbnRQb3J0KSB7XG4gIHBhcmVudFBvcnQub24oXCJtZXNzYWdlXCIsIChkYXRhKSA9PiB7XG4gICAgZ2xvYmFsVGhpcy5vbm1lc3NhZ2UoeyBkYXRhIH0pO1xuICB9KTtcbn1cblxuT2JqZWN0LmFzc2lnbihnbG9iYWxUaGlzLCB7XG4gIHNlbGY6IGdsb2JhbFRoaXMsXG4gIHJlcXVpcmUsXG4gIFdvcmtlcixcbiAgaW1wb3J0U2NyaXB0czogZnVuY3Rpb24gKGYpIHtcbiAgICA7KDAsIGV2YWwpKGZzLnJlYWRGaWxlU3luYyhmLCBcInV0ZjhcIikgKyBcIi8vIyBzb3VyY2VVUkw9XCIgKyBmKTtcbiAgfSxcbiAgcG9zdE1lc3NhZ2U6IGZ1bmN0aW9uIChtc2cpIHtcbiAgICBpZiAocGFyZW50UG9ydCkge1xuICAgICAgcGFyZW50UG9ydC5wb3N0TWVzc2FnZShtc2cpO1xuICAgIH1cbiAgfSxcbn0pO1xuXG5jb25zdCBlbW5hcGlDb250ZXh0ID0gZ2V0RGVmYXVsdENvbnRleHQoKTtcblxuY29uc3QgX19yb290RGlyID0gcGFyc2UocHJvY2Vzcy5jd2QoKSkucm9vdDtcblxuY29uc3QgaGFuZGxlciA9IG5ldyBNZXNzYWdlSGFuZGxlcih7XG4gIG9uTG9hZCh7IHdhc21Nb2R1bGUsIHdhc21NZW1vcnkgfSkge1xuICAgIGNvbnN0IHdhc2kgPSBuZXcgV0FTSSh7XG4gICAgICB2ZXJzaW9uOiAncHJldmlldzEnLFxuICAgICAgZW52OiBwcm9jZXNzLmVudixcbiAgICAgIHByZW9wZW5zOiB7XG4gICAgICAgIFtfX3Jvb3REaXJdOiBfX3Jvb3REaXIsXG4gICAgICB9LFxuICAgIH0pO1xuXG4gICAgcmV0dXJuIGluc3RhbnRpYXRlTmFwaU1vZHVsZVN5bmMod2FzbU1vZHVsZSwge1xuICAgICAgY2hpbGRUaHJlYWQ6IHRydWUsXG4gICAgICB3YXNpLFxuICAgICAgY29udGV4dDogZW1uYXBpQ29udGV4dCxcbiAgICAgIG92ZXJ3cml0ZUltcG9ydHMoaW1wb3J0T2JqZWN0KSB7XG4gICAgICAgIGltcG9ydE9iamVjdC5lbnYgPSB7XG4gICAgICAgICAgLi4uaW1wb3J0T2JqZWN0LmVudixcbiAgICAgICAgICAuLi5pbXBvcnRPYmplY3QubmFwaSxcbiAgICAgICAgICAuLi5pbXBvcnRPYmplY3QuZW1uYXBpLFxuICAgICAgICAgIG1lbW9yeTogd2FzbU1lbW9yeVxuICAgICAgICB9O1xuICAgICAgfSxcbiAgICB9KTtcbiAgfSxcbn0pO1xuXG5nbG9iYWxUaGlzLm9ubWVzc2FnZSA9IGZ1bmN0aW9uIChlKSB7XG4gIGhhbmRsZXIuaGFuZGxlKGUpO1xufTtcbmBcblxuZXhwb3J0IGNvbnN0IGNyZWF0ZVdhc2lCcm93c2VyV29ya2VyQmluZGluZyA9IChmczogYm9vbGVhbikgPT4ge1xuICBjb25zdCBmc0ltcG9ydCA9IGZzXG4gICAgPyBgaW1wb3J0IHsgaW5zdGFudGlhdGVOYXBpTW9kdWxlU3luYywgTWVzc2FnZUhhbmRsZXIsIFdBU0ksIGNyZWF0ZUZzUHJveHkgfSBmcm9tICdAbmFwaS1ycy93YXNtLXJ1bnRpbWUnXG5pbXBvcnQgeyBtZW1mc0V4cG9ydGVkIGFzIF9fbWVtZnNFeHBvcnRlZCB9IGZyb20gJ0BuYXBpLXJzL3dhc20tcnVudGltZS9mcydcblxuY29uc3QgZnMgPSBjcmVhdGVGc1Byb3h5KF9fbWVtZnNFeHBvcnRlZClgXG4gICAgOiBgaW1wb3J0IHsgaW5zdGFudGlhdGVOYXBpTW9kdWxlU3luYywgTWVzc2FnZUhhbmRsZXIsIFdBU0kgfSBmcm9tICdAbmFwaS1ycy93YXNtLXJ1bnRpbWUnYFxuICBjb25zdCB3YXNpQ3JlYXRpb24gPSBmc1xuICAgID8gYGNvbnN0IHdhc2kgPSBuZXcgV0FTSSh7XG4gICAgICBmcyxcbiAgICAgIHByZW9wZW5zOiB7XG4gICAgICAgICcvJzogJy8nLFxuICAgICAgfSxcbiAgICAgIHByaW50OiBmdW5jdGlvbiAoKSB7XG4gICAgICAgIC8vIGVzbGludC1kaXNhYmxlLW5leHQtbGluZSBuby1jb25zb2xlXG4gICAgICAgIGNvbnNvbGUubG9nLmFwcGx5KGNvbnNvbGUsIGFyZ3VtZW50cylcbiAgICAgIH0sXG4gICAgICBwcmludEVycjogZnVuY3Rpb24oKSB7XG4gICAgICAgIC8vIGVzbGludC1kaXNhYmxlLW5leHQtbGluZSBuby1jb25zb2xlXG4gICAgICAgIGNvbnNvbGUuZXJyb3IuYXBwbHkoY29uc29sZSwgYXJndW1lbnRzKVxuICAgICAgfSxcbiAgICB9KWBcbiAgICA6IGBjb25zdCB3YXNpID0gbmV3IFdBU0koe1xuICAgICAgcHJpbnQ6IGZ1bmN0aW9uICgpIHtcbiAgICAgICAgLy8gZXNsaW50LWRpc2FibGUtbmV4dC1saW5lIG5vLWNvbnNvbGVcbiAgICAgICAgY29uc29sZS5sb2cuYXBwbHkoY29uc29sZSwgYXJndW1lbnRzKVxuICAgICAgfSxcbiAgICAgIHByaW50RXJyOiBmdW5jdGlvbigpIHtcbiAgICAgICAgLy8gZXNsaW50LWRpc2FibGUtbmV4dC1saW5lIG5vLWNvbnNvbGVcbiAgICAgICAgY29uc29sZS5lcnJvci5hcHBseShjb25zb2xlLCBhcmd1bWVudHMpXG4gICAgICB9LFxuICAgIH0pYFxuICByZXR1cm4gYCR7ZnNJbXBvcnR9XG5cbmNvbnN0IGhhbmRsZXIgPSBuZXcgTWVzc2FnZUhhbmRsZXIoe1xuICBvbkxvYWQoeyB3YXNtTW9kdWxlLCB3YXNtTWVtb3J5IH0pIHtcbiAgICAke3dhc2lDcmVhdGlvbn1cbiAgICByZXR1cm4gaW5zdGFudGlhdGVOYXBpTW9kdWxlU3luYyh3YXNtTW9kdWxlLCB7XG4gICAgICBjaGlsZFRocmVhZDogdHJ1ZSxcbiAgICAgIHdhc2ksXG4gICAgICBvdmVyd3JpdGVJbXBvcnRzKGltcG9ydE9iamVjdCkge1xuICAgICAgICBpbXBvcnRPYmplY3QuZW52ID0ge1xuICAgICAgICAgIC4uLmltcG9ydE9iamVjdC5lbnYsXG4gICAgICAgICAgLi4uaW1wb3J0T2JqZWN0Lm5hcGksXG4gICAgICAgICAgLi4uaW1wb3J0T2JqZWN0LmVtbmFwaSxcbiAgICAgICAgICBtZW1vcnk6IHdhc21NZW1vcnksXG4gICAgICAgIH1cbiAgICAgIH0sXG4gICAgfSlcbiAgfSxcbn0pXG5cbmdsb2JhbFRoaXMub25tZXNzYWdlID0gZnVuY3Rpb24gKGUpIHtcbiAgaGFuZGxlci5oYW5kbGUoZSlcbn1cbmBcbn1cbiIsImltcG9ydCB7IHNwYXduIH0gZnJvbSAnbm9kZTpjaGlsZF9wcm9jZXNzJ1xuaW1wb3J0IHsgY3JlYXRlSGFzaCB9IGZyb20gJ25vZGU6Y3J5cHRvJ1xuaW1wb3J0IHsgZXhpc3RzU3luYywgbWtkaXJTeW5jLCBybVN5bmMgfSBmcm9tICdub2RlOmZzJ1xuaW1wb3J0IHsgY3JlYXRlUmVxdWlyZSB9IGZyb20gJ25vZGU6bW9kdWxlJ1xuaW1wb3J0IHsgaG9tZWRpciB9IGZyb20gJ25vZGU6b3MnXG5pbXBvcnQgeyBwYXJzZSwgam9pbiwgcmVzb2x2ZSB9IGZyb20gJ25vZGU6cGF0aCdcblxuaW1wb3J0ICogYXMgY29sb3JzIGZyb20gJ2NvbG9yZXR0ZSdcblxuaW1wb3J0IHR5cGUgeyBCdWlsZE9wdGlvbnMgYXMgUmF3QnVpbGRPcHRpb25zIH0gZnJvbSAnLi4vZGVmL2J1aWxkLmpzJ1xuaW1wb3J0IHtcbiAgQ0xJX1ZFUlNJT04sXG4gIGNvcHlGaWxlQXN5bmMsXG4gIHR5cGUgQ3JhdGUsXG4gIGRlYnVnRmFjdG9yeSxcbiAgREVGQVVMVF9UWVBFX0RFRl9IRUFERVIsXG4gIGZpbGVFeGlzdHMsXG4gIGdldFN5c3RlbURlZmF1bHRUYXJnZXQsXG4gIGdldFRhcmdldExpbmtlcixcbiAgbWtkaXJBc3luYyxcbiAgdHlwZSBOYXBpQ29uZmlnLFxuICBwYXJzZU1ldGFkYXRhLFxuICBwYXJzZVRyaXBsZSxcbiAgcHJvY2Vzc1R5cGVEZWYsXG4gIHJlYWRGaWxlQXN5bmMsXG4gIHJlYWROYXBpQ29uZmlnLFxuICB0eXBlIFRhcmdldCxcbiAgdGFyZ2V0VG9FbnZWYXIsXG4gIHRyeUluc3RhbGxDYXJnb0JpbmFyeSxcbiAgdW5saW5rQXN5bmMsXG4gIHdyaXRlRmlsZUFzeW5jLFxuICBkaXJFeGlzdHNBc3luYyxcbiAgcmVhZGRpckFzeW5jLFxuICB0eXBlIENhcmdvV29ya3NwYWNlTWV0YWRhdGEsXG59IGZyb20gJy4uL3V0aWxzL2luZGV4LmpzJ1xuXG5pbXBvcnQgeyBjcmVhdGVDanNCaW5kaW5nLCBjcmVhdGVFc21CaW5kaW5nIH0gZnJvbSAnLi90ZW1wbGF0ZXMvaW5kZXguanMnXG5pbXBvcnQge1xuICBjcmVhdGVXYXNpQmluZGluZyxcbiAgY3JlYXRlV2FzaUJyb3dzZXJCaW5kaW5nLFxufSBmcm9tICcuL3RlbXBsYXRlcy9sb2FkLXdhc2ktdGVtcGxhdGUuanMnXG5pbXBvcnQge1xuICBjcmVhdGVXYXNpQnJvd3NlcldvcmtlckJpbmRpbmcsXG4gIFdBU0lfV09SS0VSX1RFTVBMQVRFLFxufSBmcm9tICcuL3RlbXBsYXRlcy93YXNpLXdvcmtlci10ZW1wbGF0ZS5qcydcblxuY29uc3QgZGVidWcgPSBkZWJ1Z0ZhY3RvcnkoJ2J1aWxkJylcbmNvbnN0IHJlcXVpcmUgPSBjcmVhdGVSZXF1aXJlKGltcG9ydC5tZXRhLnVybClcblxudHlwZSBPdXRwdXRLaW5kID0gJ2pzJyB8ICdkdHMnIHwgJ25vZGUnIHwgJ2V4ZScgfCAnd2FzbSdcbnR5cGUgT3V0cHV0ID0geyBraW5kOiBPdXRwdXRLaW5kOyBwYXRoOiBzdHJpbmcgfVxuXG50eXBlIEJ1aWxkT3B0aW9ucyA9IFJhd0J1aWxkT3B0aW9ucyAmIHsgY2FyZ29PcHRpb25zPzogc3RyaW5nW10gfVxudHlwZSBQYXJzZWRCdWlsZE9wdGlvbnMgPSBPbWl0PEJ1aWxkT3B0aW9ucywgJ2N3ZCc+ICYgeyBjd2Q6IHN0cmluZyB9XG5cbmV4cG9ydCBhc3luYyBmdW5jdGlvbiBidWlsZFByb2plY3QocmF3T3B0aW9uczogQnVpbGRPcHRpb25zKSB7XG4gIGRlYnVnKCduYXBpIGJ1aWxkIGNvbW1hbmQgcmVjZWl2ZSBvcHRpb25zOiAlTycsIHJhd09wdGlvbnMpXG5cbiAgY29uc3Qgb3B0aW9uczogUGFyc2VkQnVpbGRPcHRpb25zID0ge1xuICAgIGR0c0NhY2hlOiB0cnVlLFxuICAgIC4uLnJhd09wdGlvbnMsXG4gICAgY3dkOiByYXdPcHRpb25zLmN3ZCA/PyBwcm9jZXNzLmN3ZCgpLFxuICB9XG5cbiAgY29uc3QgcmVzb2x2ZVBhdGggPSAoLi4ucGF0aHM6IHN0cmluZ1tdKSA9PiByZXNvbHZlKG9wdGlvbnMuY3dkLCAuLi5wYXRocylcblxuICBjb25zdCBtYW5pZmVzdFBhdGggPSByZXNvbHZlUGF0aChvcHRpb25zLm1hbmlmZXN0UGF0aCA/PyAnQ2FyZ28udG9tbCcpXG4gIGNvbnN0IG1ldGFkYXRhID0gYXdhaXQgcGFyc2VNZXRhZGF0YShtYW5pZmVzdFBhdGgpXG5cbiAgY29uc3QgY3JhdGUgPSBtZXRhZGF0YS5wYWNrYWdlcy5maW5kKChwKSA9PiB7XG4gICAgLy8gcGFja2FnZSB3aXRoIGdpdmVuIG5hbWVcbiAgICBpZiAob3B0aW9ucy5wYWNrYWdlKSB7XG4gICAgICByZXR1cm4gcC5uYW1lID09PSBvcHRpb25zLnBhY2thZ2VcbiAgICB9IGVsc2Uge1xuICAgICAgcmV0dXJuIHAubWFuaWZlc3RfcGF0aCA9PT0gbWFuaWZlc3RQYXRoXG4gICAgfVxuICB9KVxuXG4gIGlmICghY3JhdGUpIHtcbiAgICB0aHJvdyBuZXcgRXJyb3IoXG4gICAgICAnVW5hYmxlIHRvIGZpbmQgY3JhdGUgdG8gYnVpbGQuIEl0IHNlZW1zIHlvdSBhcmUgdHJ5aW5nIHRvIGJ1aWxkIGEgY3JhdGUgaW4gYSB3b3Jrc3BhY2UsIHRyeSB1c2luZyBgLS1wYWNrYWdlYCBvcHRpb24gdG8gc3BlY2lmeSB0aGUgcGFja2FnZSB0byBidWlsZC4nLFxuICAgIClcbiAgfVxuICBjb25zdCBjb25maWcgPSBhd2FpdCByZWFkTmFwaUNvbmZpZyhcbiAgICByZXNvbHZlUGF0aChvcHRpb25zLnBhY2thZ2VKc29uUGF0aCA/PyAncGFja2FnZS5qc29uJyksXG4gICAgb3B0aW9ucy5jb25maWdQYXRoID8gcmVzb2x2ZVBhdGgob3B0aW9ucy5jb25maWdQYXRoKSA6IHVuZGVmaW5lZCxcbiAgKVxuXG4gIGNvbnN0IGJ1aWxkZXIgPSBuZXcgQnVpbGRlcihtZXRhZGF0YSwgY3JhdGUsIGNvbmZpZywgb3B0aW9ucylcblxuICByZXR1cm4gYnVpbGRlci5idWlsZCgpXG59XG5cbmNsYXNzIEJ1aWxkZXIge1xuICBwcml2YXRlIHJlYWRvbmx5IGFyZ3M6IHN0cmluZ1tdID0gW11cbiAgcHJpdmF0ZSByZWFkb25seSBlbnZzOiBSZWNvcmQ8c3RyaW5nLCBzdHJpbmc+ID0ge31cbiAgcHJpdmF0ZSByZWFkb25seSBvdXRwdXRzOiBPdXRwdXRbXSA9IFtdXG5cbiAgcHJpdmF0ZSByZWFkb25seSB0YXJnZXQ6IFRhcmdldFxuICBwcml2YXRlIHJlYWRvbmx5IGNyYXRlRGlyOiBzdHJpbmdcbiAgcHJpdmF0ZSByZWFkb25seSBvdXRwdXREaXI6IHN0cmluZ1xuICBwcml2YXRlIHJlYWRvbmx5IHRhcmdldERpcjogc3RyaW5nXG4gIHByaXZhdGUgcmVhZG9ubHkgZW5hYmxlVHlwZURlZjogYm9vbGVhbiA9IGZhbHNlXG5cbiAgY29uc3RydWN0b3IoXG4gICAgcHJpdmF0ZSByZWFkb25seSBtZXRhZGF0YTogQ2FyZ29Xb3Jrc3BhY2VNZXRhZGF0YSxcbiAgICBwcml2YXRlIHJlYWRvbmx5IGNyYXRlOiBDcmF0ZSxcbiAgICBwcml2YXRlIHJlYWRvbmx5IGNvbmZpZzogTmFwaUNvbmZpZyxcbiAgICBwcml2YXRlIHJlYWRvbmx5IG9wdGlvbnM6IFBhcnNlZEJ1aWxkT3B0aW9ucyxcbiAgKSB7XG4gICAgdGhpcy50YXJnZXQgPSBvcHRpb25zLnRhcmdldFxuICAgICAgPyBwYXJzZVRyaXBsZShvcHRpb25zLnRhcmdldClcbiAgICAgIDogcHJvY2Vzcy5lbnYuQ0FSR09fQlVJTERfVEFSR0VUXG4gICAgICAgID8gcGFyc2VUcmlwbGUocHJvY2Vzcy5lbnYuQ0FSR09fQlVJTERfVEFSR0VUKVxuICAgICAgICA6IGdldFN5c3RlbURlZmF1bHRUYXJnZXQoKVxuICAgIHRoaXMuY3JhdGVEaXIgPSBwYXJzZShjcmF0ZS5tYW5pZmVzdF9wYXRoKS5kaXJcbiAgICB0aGlzLm91dHB1dERpciA9IHJlc29sdmUoXG4gICAgICB0aGlzLm9wdGlvbnMuY3dkLFxuICAgICAgb3B0aW9ucy5vdXRwdXREaXIgPz8gdGhpcy5jcmF0ZURpcixcbiAgICApXG4gICAgdGhpcy50YXJnZXREaXIgPVxuICAgICAgb3B0aW9ucy50YXJnZXREaXIgPz9cbiAgICAgIHByb2Nlc3MuZW52LkNBUkdPX0JVSUxEX1RBUkdFVF9ESVIgPz9cbiAgICAgIG1ldGFkYXRhLnRhcmdldF9kaXJlY3RvcnlcbiAgICB0aGlzLmVuYWJsZVR5cGVEZWYgPSB0aGlzLmNyYXRlLmRlcGVuZGVuY2llcy5zb21lKFxuICAgICAgKGRlcCkgPT5cbiAgICAgICAgZGVwLm5hbWUgPT09ICduYXBpLWRlcml2ZScgJiZcbiAgICAgICAgKGRlcC51c2VzX2RlZmF1bHRfZmVhdHVyZXMgfHwgZGVwLmZlYXR1cmVzLmluY2x1ZGVzKCd0eXBlLWRlZicpKSxcbiAgICApXG5cbiAgICBpZiAoIXRoaXMuZW5hYmxlVHlwZURlZikge1xuICAgICAgY29uc3QgcmVxdWlyZW1lbnRXYXJuaW5nID1cbiAgICAgICAgJ2BuYXBpLWRlcml2ZWAgY3JhdGUgaXMgbm90IHVzZWQgb3IgYHR5cGUtZGVmYCBmZWF0dXJlIGlzIG5vdCBlbmFibGVkIGZvciBgbmFwaS1kZXJpdmVgIGNyYXRlJ1xuICAgICAgZGVidWcud2FybihcbiAgICAgICAgYCR7cmVxdWlyZW1lbnRXYXJuaW5nfS4gV2lsbCBza2lwIGJpbmRpbmcgZ2VuZXJhdGlvbiBmb3IgXFxgLm5vZGVcXGAsIFxcYC53YXNpXFxgIGFuZCBcXGAuZC50c1xcYCBmaWxlcy5gLFxuICAgICAgKVxuXG4gICAgICBpZiAoXG4gICAgICAgIHRoaXMub3B0aW9ucy5kdHMgfHxcbiAgICAgICAgdGhpcy5vcHRpb25zLmR0c0hlYWRlciB8fFxuICAgICAgICB0aGlzLmNvbmZpZy5kdHNIZWFkZXIgfHxcbiAgICAgICAgdGhpcy5jb25maWcuZHRzSGVhZGVyRmlsZVxuICAgICAgKSB7XG4gICAgICAgIGRlYnVnLndhcm4oXG4gICAgICAgICAgYCR7cmVxdWlyZW1lbnRXYXJuaW5nfS4gXFxgZHRzXFxgIHJlbGF0ZWQgb3B0aW9ucyBhcmUgZW5hYmxlZCBidXQgd2lsbCBiZSBpZ25vcmVkLmAsXG4gICAgICAgIClcbiAgICAgIH1cbiAgICB9XG4gIH1cblxuICBnZXQgY2R5TGliTmFtZSgpIHtcbiAgICByZXR1cm4gdGhpcy5jcmF0ZS50YXJnZXRzLmZpbmQoKHQpID0+IHQuY3JhdGVfdHlwZXMuaW5jbHVkZXMoJ2NkeWxpYicpKVxuICAgICAgPy5uYW1lXG4gIH1cblxuICBnZXQgYmluTmFtZSgpIHtcbiAgICByZXR1cm4gKFxuICAgICAgdGhpcy5vcHRpb25zLmJpbiA/P1xuICAgICAgLy8gb25seSBhdmFpbGFibGUgaWYgbm90IGNkeWxpYiBvciBiaW4gbmFtZSBzcGVjaWZpZWRcbiAgICAgICh0aGlzLmNkeUxpYk5hbWVcbiAgICAgICAgPyBudWxsXG4gICAgICAgIDogdGhpcy5jcmF0ZS50YXJnZXRzLmZpbmQoKHQpID0+IHQuY3JhdGVfdHlwZXMuaW5jbHVkZXMoJ2JpbicpKT8ubmFtZSlcbiAgICApXG4gIH1cblxuICBidWlsZCgpIHtcbiAgICBpZiAoIXRoaXMuY2R5TGliTmFtZSkge1xuICAgICAgY29uc3Qgd2FybmluZyA9XG4gICAgICAgICdNaXNzaW5nIGBjcmF0ZS10eXBlID0gW1wiY2R5bGliXCJdYCBpbiBbbGliXSBjb25maWcuIFRoZSBidWlsZCByZXN1bHQgd2lsbCBub3QgYmUgYXZhaWxhYmxlIGFzIG5vZGUgYWRkb24uJ1xuXG4gICAgICBpZiAodGhpcy5iaW5OYW1lKSB7XG4gICAgICAgIGRlYnVnLndhcm4od2FybmluZylcbiAgICAgIH0gZWxzZSB7XG4gICAgICAgIHRocm93IG5ldyBFcnJvcih3YXJuaW5nKVxuICAgICAgfVxuICAgIH1cblxuICAgIHJldHVybiB0aGlzLnBpY2tCaW5hcnkoKVxuICAgICAgLnNldFBhY2thZ2UoKVxuICAgICAgLnNldEZlYXR1cmVzKClcbiAgICAgIC5zZXRUYXJnZXQoKVxuICAgICAgLnBpY2tDcm9zc1Rvb2xjaGFpbigpXG4gICAgICAuc2V0RW52cygpXG4gICAgICAuc2V0QnlwYXNzQXJncygpXG4gICAgICAuZXhlYygpXG4gIH1cblxuICBwcml2YXRlIHBpY2tDcm9zc1Rvb2xjaGFpbigpIHtcbiAgICBpZiAoIXRoaXMub3B0aW9ucy51c2VOYXBpQ3Jvc3MpIHtcbiAgICAgIHJldHVybiB0aGlzXG4gICAgfVxuICAgIGlmICh0aGlzLm9wdGlvbnMudXNlQ3Jvc3MpIHtcbiAgICAgIGRlYnVnLndhcm4oXG4gICAgICAgICdZb3UgYXJlIHRyeWluZyB0byB1c2UgYm90aCBgLS1jcm9zc2AgYW5kIGAtLXVzZS1uYXBpLWNyb3NzYCBvcHRpb25zLCBgLS11c2UtY3Jvc3NgIHdpbGwgYmUgaWdub3JlZC4nLFxuICAgICAgKVxuICAgIH1cblxuICAgIGlmICh0aGlzLm9wdGlvbnMuY3Jvc3NDb21waWxlKSB7XG4gICAgICBkZWJ1Zy53YXJuKFxuICAgICAgICAnWW91IGFyZSB0cnlpbmcgdG8gdXNlIGJvdGggYC0tY3Jvc3MtY29tcGlsZWAgYW5kIGAtLXVzZS1uYXBpLWNyb3NzYCBvcHRpb25zLCBgLS1jcm9zcy1jb21waWxlYCB3aWxsIGJlIGlnbm9yZWQuJyxcbiAgICAgIClcbiAgICB9XG5cbiAgICB0cnkge1xuICAgICAgY29uc3QgeyB2ZXJzaW9uLCBkb3dubG9hZCB9ID0gcmVxdWlyZSgnQG5hcGktcnMvY3Jvc3MtdG9vbGNoYWluJylcblxuICAgICAgY29uc3QgYWxpYXM6IFJlY29yZDxzdHJpbmcsIHN0cmluZz4gPSB7XG4gICAgICAgICdzMzkweC11bmtub3duLWxpbnV4LWdudSc6ICdzMzkweC1pYm0tbGludXgtZ251JyxcbiAgICAgIH1cblxuICAgICAgY29uc3QgdG9vbGNoYWluUGF0aCA9IGpvaW4oXG4gICAgICAgIGhvbWVkaXIoKSxcbiAgICAgICAgJy5uYXBpLXJzJyxcbiAgICAgICAgJ2Nyb3NzLXRvb2xjaGFpbicsXG4gICAgICAgIHZlcnNpb24sXG4gICAgICAgIHRoaXMudGFyZ2V0LnRyaXBsZSxcbiAgICAgIClcbiAgICAgIG1rZGlyU3luYyh0b29sY2hhaW5QYXRoLCB7IHJlY3Vyc2l2ZTogdHJ1ZSB9KVxuICAgICAgaWYgKGV4aXN0c1N5bmMoam9pbih0b29sY2hhaW5QYXRoLCAncGFja2FnZS5qc29uJykpKSB7XG4gICAgICAgIGRlYnVnKGBUb29sY2hhaW4gJHt0b29sY2hhaW5QYXRofSBleGlzdHMsIHNraXAgZXh0cmFjdGluZ2ApXG4gICAgICB9IGVsc2Uge1xuICAgICAgICBjb25zdCB0YXJBcmNoaXZlID0gZG93bmxvYWQocHJvY2Vzcy5hcmNoLCB0aGlzLnRhcmdldC50cmlwbGUpXG4gICAgICAgIHRhckFyY2hpdmUudW5wYWNrKHRvb2xjaGFpblBhdGgpXG4gICAgICB9XG4gICAgICBjb25zdCB1cHBlckNhc2VUYXJnZXQgPSB0YXJnZXRUb0VudlZhcih0aGlzLnRhcmdldC50cmlwbGUpXG4gICAgICBjb25zdCBjcm9zc1RhcmdldE5hbWUgPSBhbGlhc1t0aGlzLnRhcmdldC50cmlwbGVdID8/IHRoaXMudGFyZ2V0LnRyaXBsZVxuICAgICAgY29uc3QgbGlua2VyRW52ID0gYENBUkdPX1RBUkdFVF8ke3VwcGVyQ2FzZVRhcmdldH1fTElOS0VSYFxuICAgICAgdGhpcy5zZXRFbnZJZk5vdEV4aXN0cyhcbiAgICAgICAgbGlua2VyRW52LFxuICAgICAgICBqb2luKHRvb2xjaGFpblBhdGgsICdiaW4nLCBgJHtjcm9zc1RhcmdldE5hbWV9LWdjY2ApLFxuICAgICAgKVxuICAgICAgdGhpcy5zZXRFbnZJZk5vdEV4aXN0cyhcbiAgICAgICAgJ1RBUkdFVF9TWVNST09UJyxcbiAgICAgICAgam9pbih0b29sY2hhaW5QYXRoLCBjcm9zc1RhcmdldE5hbWUsICdzeXNyb290JyksXG4gICAgICApXG4gICAgICB0aGlzLnNldEVudklmTm90RXhpc3RzKFxuICAgICAgICAnVEFSR0VUX0FSJyxcbiAgICAgICAgam9pbih0b29sY2hhaW5QYXRoLCAnYmluJywgYCR7Y3Jvc3NUYXJnZXROYW1lfS1hcmApLFxuICAgICAgKVxuICAgICAgdGhpcy5zZXRFbnZJZk5vdEV4aXN0cyhcbiAgICAgICAgJ1RBUkdFVF9SQU5MSUInLFxuICAgICAgICBqb2luKHRvb2xjaGFpblBhdGgsICdiaW4nLCBgJHtjcm9zc1RhcmdldE5hbWV9LXJhbmxpYmApLFxuICAgICAgKVxuICAgICAgdGhpcy5zZXRFbnZJZk5vdEV4aXN0cyhcbiAgICAgICAgJ1RBUkdFVF9SRUFERUxGJyxcbiAgICAgICAgam9pbih0b29sY2hhaW5QYXRoLCAnYmluJywgYCR7Y3Jvc3NUYXJnZXROYW1lfS1yZWFkZWxmYCksXG4gICAgICApXG4gICAgICB0aGlzLnNldEVudklmTm90RXhpc3RzKFxuICAgICAgICAnVEFSR0VUX0NfSU5DTFVERV9QQVRIJyxcbiAgICAgICAgam9pbih0b29sY2hhaW5QYXRoLCBjcm9zc1RhcmdldE5hbWUsICdzeXNyb290JywgJ3VzcicsICdpbmNsdWRlLycpLFxuICAgICAgKVxuICAgICAgdGhpcy5zZXRFbnZJZk5vdEV4aXN0cyhcbiAgICAgICAgJ1RBUkdFVF9DQycsXG4gICAgICAgIGpvaW4odG9vbGNoYWluUGF0aCwgJ2JpbicsIGAke2Nyb3NzVGFyZ2V0TmFtZX0tZ2NjYCksXG4gICAgICApXG4gICAgICB0aGlzLnNldEVudklmTm90RXhpc3RzKFxuICAgICAgICAnVEFSR0VUX0NYWCcsXG4gICAgICAgIGpvaW4odG9vbGNoYWluUGF0aCwgJ2JpbicsIGAke2Nyb3NzVGFyZ2V0TmFtZX0tZysrYCksXG4gICAgICApXG4gICAgICB0aGlzLnNldEVudklmTm90RXhpc3RzKFxuICAgICAgICAnQklOREdFTl9FWFRSQV9DTEFOR19BUkdTJyxcbiAgICAgICAgYC0tc3lzcm9vdD0ke3RoaXMuZW52cy5UQVJHRVRfU1lTUk9PVH19YCxcbiAgICAgIClcblxuICAgICAgaWYgKFxuICAgICAgICBwcm9jZXNzLmVudi5UQVJHRVRfQ0M/LnN0YXJ0c1dpdGgoJ2NsYW5nJykgfHxcbiAgICAgICAgKHByb2Nlc3MuZW52LkNDPy5zdGFydHNXaXRoKCdjbGFuZycpICYmICFwcm9jZXNzLmVudi5UQVJHRVRfQ0MpXG4gICAgICApIHtcbiAgICAgICAgY29uc3QgVEFSR0VUX0NGTEFHUyA9IHByb2Nlc3MuZW52LlRBUkdFVF9DRkxBR1MgPz8gJydcbiAgICAgICAgdGhpcy5lbnZzLlRBUkdFVF9DRkxBR1MgPSBgLS1zeXNyb290PSR7dGhpcy5lbnZzLlRBUkdFVF9TWVNST09UfSAtLWdjYy10b29sY2hhaW49JHt0b29sY2hhaW5QYXRofSAke1RBUkdFVF9DRkxBR1N9YFxuICAgICAgfVxuICAgICAgaWYgKFxuICAgICAgICAocHJvY2Vzcy5lbnYuQ1hYPy5zdGFydHNXaXRoKCdjbGFuZysrJykgJiYgIXByb2Nlc3MuZW52LlRBUkdFVF9DWFgpIHx8XG4gICAgICAgIHByb2Nlc3MuZW52LlRBUkdFVF9DWFg/LnN0YXJ0c1dpdGgoJ2NsYW5nKysnKVxuICAgICAgKSB7XG4gICAgICAgIGNvbnN0IFRBUkdFVF9DWFhGTEFHUyA9IHByb2Nlc3MuZW52LlRBUkdFVF9DWFhGTEFHUyA/PyAnJ1xuICAgICAgICB0aGlzLmVudnMuVEFSR0VUX0NYWEZMQUdTID0gYC0tc3lzcm9vdD0ke3RoaXMuZW52cy5UQVJHRVRfU1lTUk9PVH0gLS1nY2MtdG9vbGNoYWluPSR7dG9vbGNoYWluUGF0aH0gJHtUQVJHRVRfQ1hYRkxBR1N9YFxuICAgICAgfVxuICAgICAgdGhpcy5lbnZzLlBBVEggPSB0aGlzLmVudnMuUEFUSFxuICAgICAgICA/IGAke3Rvb2xjaGFpblBhdGh9L2Jpbjoke3RoaXMuZW52cy5QQVRIfToke3Byb2Nlc3MuZW52LlBBVEh9YFxuICAgICAgICA6IGAke3Rvb2xjaGFpblBhdGh9L2Jpbjoke3Byb2Nlc3MuZW52LlBBVEh9YFxuICAgIH0gY2F0Y2ggKGUpIHtcbiAgICAgIGRlYnVnLndhcm4oJ1BpY2sgY3Jvc3MgdG9vbGNoYWluIGZhaWxlZCcsIGUgYXMgRXJyb3IpXG4gICAgICAvLyBpZ25vcmUsIGRvIG5vdGhpbmdcbiAgICB9XG4gICAgcmV0dXJuIHRoaXNcbiAgfVxuXG4gIHByaXZhdGUgZXhlYygpIHtcbiAgICBkZWJ1ZyhgU3RhcnQgYnVpbGRpbmcgY3JhdGU6ICR7dGhpcy5jcmF0ZS5uYW1lfWApXG4gICAgZGVidWcoJyAgJWknLCBgY2FyZ28gJHt0aGlzLmFyZ3Muam9pbignICcpfWApXG5cbiAgICBjb25zdCBjb250cm9sbGVyID0gbmV3IEFib3J0Q29udHJvbGxlcigpXG5cbiAgICBjb25zdCB3YXRjaCA9IHRoaXMub3B0aW9ucy53YXRjaFxuICAgIGNvbnN0IGJ1aWxkVGFzayA9IG5ldyBQcm9taXNlPHZvaWQ+KChyZXNvbHZlLCByZWplY3QpID0+IHtcbiAgICAgIGlmICh0aGlzLm9wdGlvbnMudXNlQ3Jvc3MgJiYgdGhpcy5vcHRpb25zLmNyb3NzQ29tcGlsZSkge1xuICAgICAgICB0aHJvdyBuZXcgRXJyb3IoXG4gICAgICAgICAgJ2AtLXVzZS1jcm9zc2AgYW5kIGAtLWNyb3NzLWNvbXBpbGVgIGNhbiBub3QgYmUgdXNlZCB0b2dldGhlcicsXG4gICAgICAgIClcbiAgICAgIH1cbiAgICAgIGNvbnN0IGNvbW1hbmQgPVxuICAgICAgICBwcm9jZXNzLmVudi5DQVJHTyA/PyAodGhpcy5vcHRpb25zLnVzZUNyb3NzID8gJ2Nyb3NzJyA6ICdjYXJnbycpXG4gICAgICBjb25zdCBidWlsZFByb2Nlc3MgPSBzcGF3bihjb21tYW5kLCB0aGlzLmFyZ3MsIHtcbiAgICAgICAgZW52OiB7IC4uLnByb2Nlc3MuZW52LCAuLi50aGlzLmVudnMgfSxcbiAgICAgICAgc3RkaW86IHdhdGNoID8gWydpbmhlcml0JywgJ2luaGVyaXQnLCAncGlwZSddIDogJ2luaGVyaXQnLFxuICAgICAgICBjd2Q6IHRoaXMub3B0aW9ucy5jd2QsXG4gICAgICAgIHNpZ25hbDogY29udHJvbGxlci5zaWduYWwsXG4gICAgICB9KVxuXG4gICAgICBidWlsZFByb2Nlc3Mub25jZSgnZXhpdCcsIChjb2RlKSA9PiB7XG4gICAgICAgIGlmIChjb2RlID09PSAwKSB7XG4gICAgICAgICAgZGVidWcoJyVpJywgYEJ1aWxkIGNyYXRlICR7dGhpcy5jcmF0ZS5uYW1lfSBzdWNjZXNzZnVsbHkhYClcbiAgICAgICAgICByZXNvbHZlKClcbiAgICAgICAgfSBlbHNlIHtcbiAgICAgICAgICByZWplY3QobmV3IEVycm9yKGBCdWlsZCBmYWlsZWQgd2l0aCBleGl0IGNvZGUgJHtjb2RlfWApKVxuICAgICAgICB9XG4gICAgICB9KVxuXG4gICAgICBidWlsZFByb2Nlc3Mub25jZSgnZXJyb3InLCAoZSkgPT4ge1xuICAgICAgICByZWplY3QobmV3IEVycm9yKGBCdWlsZCBmYWlsZWQgd2l0aCBlcnJvcjogJHtlLm1lc3NhZ2V9YCwgeyBjYXVzZTogZSB9KSlcbiAgICAgIH0pXG5cbiAgICAgIC8vIHdhdGNoIG1vZGUgb25seSwgdGhleSBhcmUgcGlwZWQgdGhyb3VnaCBzdGRlcnJcbiAgICAgIGJ1aWxkUHJvY2Vzcy5zdGRlcnI/Lm9uKCdkYXRhJywgKGRhdGEpID0+IHtcbiAgICAgICAgY29uc3Qgb3V0cHV0ID0gZGF0YS50b1N0cmluZygpXG4gICAgICAgIGNvbnNvbGUuZXJyb3Iob3V0cHV0KVxuICAgICAgICBpZiAoL0ZpbmlzaGVkXFxzKGBkZXZgfGByZWxlYXNlYCkvLnRlc3Qob3V0cHV0KSkge1xuICAgICAgICAgIHRoaXMucG9zdEJ1aWxkKCkuY2F0Y2goKCkgPT4ge30pXG4gICAgICAgIH1cbiAgICAgIH0pXG4gICAgfSlcblxuICAgIHJldHVybiB7XG4gICAgICB0YXNrOiBidWlsZFRhc2sudGhlbigoKSA9PiB0aGlzLnBvc3RCdWlsZCgpKSxcbiAgICAgIGFib3J0OiAoKSA9PiBjb250cm9sbGVyLmFib3J0KCksXG4gICAgfVxuICB9XG5cbiAgcHJpdmF0ZSBwaWNrQmluYXJ5KCkge1xuICAgIGxldCBzZXQgPSBmYWxzZVxuICAgIGlmICh0aGlzLm9wdGlvbnMud2F0Y2gpIHtcbiAgICAgIGlmIChwcm9jZXNzLmVudi5DSSkge1xuICAgICAgICBkZWJ1Zy53YXJuKCdXYXRjaCBtb2RlIGlzIG5vdCBzdXBwb3J0ZWQgaW4gQ0kgZW52aXJvbm1lbnQnKVxuICAgICAgfSBlbHNlIHtcbiAgICAgICAgZGVidWcoJ1VzZSAlaScsICdjYXJnby13YXRjaCcpXG4gICAgICAgIHRyeUluc3RhbGxDYXJnb0JpbmFyeSgnY2FyZ28td2F0Y2gnLCAnd2F0Y2gnKVxuICAgICAgICAvLyB5YXJuIG5hcGkgd2F0Y2ggLS10YXJnZXQgeDg2XzY0LXVua25vd24tbGludXgtZ251IFstLWNyb3NzLWNvbXBpbGVdXG4gICAgICAgIC8vID09PT5cbiAgICAgICAgLy8gY2FyZ28gd2F0Y2ggWy4uLl0gLS0gYnVpbGQgLS10YXJnZXQgeDg2XzY0LXVua25vd24tbGludXgtZ251XG4gICAgICAgIC8vIGNhcmdvIHdhdGNoIFsuLi5dIC0tIHppZ2J1aWxkIC0tdGFyZ2V0IHg4Nl82NC11bmtub3duLWxpbnV4LWdudVxuICAgICAgICB0aGlzLmFyZ3MucHVzaChcbiAgICAgICAgICAnd2F0Y2gnLFxuICAgICAgICAgICctLXdoeScsXG4gICAgICAgICAgJy1pJyxcbiAgICAgICAgICAnKi57anMsdHMsbm9kZX0nLFxuICAgICAgICAgICctdycsXG4gICAgICAgICAgdGhpcy5jcmF0ZURpcixcbiAgICAgICAgICAnLS0nLFxuICAgICAgICAgICdjYXJnbycsXG4gICAgICAgICAgJ2J1aWxkJyxcbiAgICAgICAgKVxuICAgICAgICBzZXQgPSB0cnVlXG4gICAgICB9XG4gICAgfVxuXG4gICAgaWYgKHRoaXMub3B0aW9ucy5jcm9zc0NvbXBpbGUpIHtcbiAgICAgIGlmICh0aGlzLnRhcmdldC5wbGF0Zm9ybSA9PT0gJ3dpbjMyJykge1xuICAgICAgICBpZiAocHJvY2Vzcy5wbGF0Zm9ybSA9PT0gJ3dpbjMyJykge1xuICAgICAgICAgIGRlYnVnLndhcm4oXG4gICAgICAgICAgICAnWW91IGFyZSB0cnlpbmcgdG8gY3Jvc3MgY29tcGlsZSB0byB3aW4zMiBwbGF0Zm9ybSBvbiB3aW4zMiBwbGF0Zm9ybSB3aGljaCBpcyB1bm5lY2Vzc2FyeS4nLFxuICAgICAgICAgIClcbiAgICAgICAgfSBlbHNlIHtcbiAgICAgICAgICAvLyB1c2UgY2FyZ28teHdpbiB0byBjcm9zcyBjb21waWxlIHRvIHdpbjMyIHBsYXRmb3JtXG4gICAgICAgICAgZGVidWcoJ1VzZSAlaScsICdjYXJnby14d2luJylcbiAgICAgICAgICB0cnlJbnN0YWxsQ2FyZ29CaW5hcnkoJ2NhcmdvLXh3aW4nLCAneHdpbicpXG4gICAgICAgICAgdGhpcy5hcmdzLnB1c2goJ3h3aW4nLCAnYnVpbGQnKVxuICAgICAgICAgIGlmICh0aGlzLnRhcmdldC5hcmNoID09PSAnaWEzMicpIHtcbiAgICAgICAgICAgIHRoaXMuZW52cy5YV0lOX0FSQ0ggPSAneDg2J1xuICAgICAgICAgIH1cbiAgICAgICAgICBzZXQgPSB0cnVlXG4gICAgICAgIH1cbiAgICAgIH0gZWxzZSB7XG4gICAgICAgIGlmIChcbiAgICAgICAgICB0aGlzLnRhcmdldC5wbGF0Zm9ybSA9PT0gJ2xpbnV4JyAmJlxuICAgICAgICAgIHByb2Nlc3MucGxhdGZvcm0gPT09ICdsaW51eCcgJiZcbiAgICAgICAgICB0aGlzLnRhcmdldC5hcmNoID09PSBwcm9jZXNzLmFyY2ggJiZcbiAgICAgICAgICAoZnVuY3Rpb24gKGFiaTogc3RyaW5nIHwgbnVsbCkge1xuICAgICAgICAgICAgY29uc3QgZ2xpYmNWZXJzaW9uUnVudGltZSA9XG4gICAgICAgICAgICAgIC8vIEB0cy1leHBlY3QtZXJyb3JcbiAgICAgICAgICAgICAgcHJvY2Vzcy5yZXBvcnQ/LmdldFJlcG9ydCgpPy5oZWFkZXI/LmdsaWJjVmVyc2lvblJ1bnRpbWVcbiAgICAgICAgICAgIGNvbnN0IGxpYmMgPSBnbGliY1ZlcnNpb25SdW50aW1lID8gJ2dudScgOiAnbXVzbCdcbiAgICAgICAgICAgIHJldHVybiBhYmkgPT09IGxpYmNcbiAgICAgICAgICB9KSh0aGlzLnRhcmdldC5hYmkpXG4gICAgICAgICkge1xuICAgICAgICAgIGRlYnVnLndhcm4oXG4gICAgICAgICAgICAnWW91IGFyZSB0cnlpbmcgdG8gY3Jvc3MgY29tcGlsZSB0byBsaW51eCB0YXJnZXQgb24gbGludXggcGxhdGZvcm0gd2hpY2ggaXMgdW5uZWNlc3NhcnkuJyxcbiAgICAgICAgICApXG4gICAgICAgIH0gZWxzZSBpZiAoXG4gICAgICAgICAgdGhpcy50YXJnZXQucGxhdGZvcm0gPT09ICdkYXJ3aW4nICYmXG4gICAgICAgICAgcHJvY2Vzcy5wbGF0Zm9ybSA9PT0gJ2RhcndpbidcbiAgICAgICAgKSB7XG4gICAgICAgICAgZGVidWcud2FybihcbiAgICAgICAgICAgICdZb3UgYXJlIHRyeWluZyB0byBjcm9zcyBjb21waWxlIHRvIGRhcndpbiB0YXJnZXQgb24gZGFyd2luIHBsYXRmb3JtIHdoaWNoIGlzIHVubmVjZXNzYXJ5LicsXG4gICAgICAgICAgKVxuICAgICAgICB9IGVsc2Uge1xuICAgICAgICAgIC8vIHVzZSBjYXJnby16aWdidWlsZCB0byBjcm9zcyBjb21waWxlIHRvIG90aGVyIHBsYXRmb3Jtc1xuICAgICAgICAgIGRlYnVnKCdVc2UgJWknLCAnY2FyZ28temlnYnVpbGQnKVxuICAgICAgICAgIHRyeUluc3RhbGxDYXJnb0JpbmFyeSgnY2FyZ28temlnYnVpbGQnLCAnemlnYnVpbGQnKVxuICAgICAgICAgIHRoaXMuYXJncy5wdXNoKCd6aWdidWlsZCcpXG4gICAgICAgICAgc2V0ID0gdHJ1ZVxuICAgICAgICB9XG4gICAgICB9XG4gICAgfVxuXG4gICAgaWYgKCFzZXQpIHtcbiAgICAgIHRoaXMuYXJncy5wdXNoKCdidWlsZCcpXG4gICAgfVxuICAgIHJldHVybiB0aGlzXG4gIH1cblxuICBwcml2YXRlIHNldFBhY2thZ2UoKSB7XG4gICAgY29uc3QgYXJncyA9IFtdXG5cbiAgICBpZiAodGhpcy5vcHRpb25zLnBhY2thZ2UpIHtcbiAgICAgIGFyZ3MucHVzaCgnLS1wYWNrYWdlJywgdGhpcy5vcHRpb25zLnBhY2thZ2UpXG4gICAgfVxuXG4gICAgaWYgKHRoaXMuYmluTmFtZSkge1xuICAgICAgYXJncy5wdXNoKCctLWJpbicsIHRoaXMuYmluTmFtZSlcbiAgICB9XG5cbiAgICBpZiAoYXJncy5sZW5ndGgpIHtcbiAgICAgIGRlYnVnKCdTZXQgcGFja2FnZSBmbGFnczogJylcbiAgICAgIGRlYnVnKCcgICVPJywgYXJncylcbiAgICAgIHRoaXMuYXJncy5wdXNoKC4uLmFyZ3MpXG4gICAgfVxuXG4gICAgcmV0dXJuIHRoaXNcbiAgfVxuXG4gIHByaXZhdGUgc2V0VGFyZ2V0KCkge1xuICAgIGRlYnVnKCdTZXQgY29tcGlsaW5nIHRhcmdldCB0bzogJylcbiAgICBkZWJ1ZygnICAlaScsIHRoaXMudGFyZ2V0LnRyaXBsZSlcblxuICAgIHRoaXMuYXJncy5wdXNoKCctLXRhcmdldCcsIHRoaXMudGFyZ2V0LnRyaXBsZSlcblxuICAgIHJldHVybiB0aGlzXG4gIH1cblxuICBwcml2YXRlIHNldEVudnMoKSB7XG4gICAgLy8gVFlQRSBERUZcbiAgICBpZiAodGhpcy5lbmFibGVUeXBlRGVmKSB7XG4gICAgICB0aGlzLmVudnMuTkFQSV9UWVBFX0RFRl9UTVBfRk9MREVSID1cbiAgICAgICAgdGhpcy5nZW5lcmF0ZUludGVybWVkaWF0ZVR5cGVEZWZGb2xkZXIoKVxuICAgICAgdGhpcy5zZXRGb3JjZUJ1aWxkRW52cyh0aGlzLmVudnMuTkFQSV9UWVBFX0RFRl9UTVBfRk9MREVSKVxuICAgIH1cblxuICAgIC8vIFJVU1RGTEFHU1xuICAgIGxldCBydXN0ZmxhZ3MgPVxuICAgICAgcHJvY2Vzcy5lbnYuUlVTVEZMQUdTID8/IHByb2Nlc3MuZW52LkNBUkdPX0JVSUxEX1JVU1RGTEFHUyA/PyAnJ1xuXG4gICAgaWYgKFxuICAgICAgdGhpcy50YXJnZXQuYWJpPy5pbmNsdWRlcygnbXVzbCcpICYmXG4gICAgICAhcnVzdGZsYWdzLmluY2x1ZGVzKCd0YXJnZXQtZmVhdHVyZT0tY3J0LXN0YXRpYycpXG4gICAgKSB7XG4gICAgICBydXN0ZmxhZ3MgKz0gJyAtQyB0YXJnZXQtZmVhdHVyZT0tY3J0LXN0YXRpYydcbiAgICB9XG5cbiAgICBpZiAodGhpcy5vcHRpb25zLnN0cmlwICYmICFydXN0ZmxhZ3MuaW5jbHVkZXMoJ2xpbmstYXJnPS1zJykpIHtcbiAgICAgIHJ1c3RmbGFncyArPSAnIC1DIGxpbmstYXJnPS1zJ1xuICAgIH1cblxuICAgIGlmIChydXN0ZmxhZ3MubGVuZ3RoKSB7XG4gICAgICB0aGlzLmVudnMuUlVTVEZMQUdTID0gcnVzdGZsYWdzXG4gICAgfVxuICAgIC8vIEVORCBSVVNURkxBR1NcblxuICAgIC8vIExJTktFUlxuICAgIGNvbnN0IGxpbmtlciA9IHRoaXMub3B0aW9ucy5jcm9zc0NvbXBpbGVcbiAgICAgID8gdm9pZCAwXG4gICAgICA6IGdldFRhcmdldExpbmtlcih0aGlzLnRhcmdldC50cmlwbGUpXG4gICAgLy8gVE9ETzpcbiAgICAvLyAgIGRpcmVjdGx5IHNldCBDQVJHT19UQVJHRVRfPHRhcmdldD5fTElOS0VSIHdpbGwgY292ZXIgLmNhcmdvL2NvbmZpZy50b21sXG4gICAgLy8gICB3aWxsIGRldGVjdCBieSBjYXJnbyBjb25maWcgd2hlbiBpdCBiZWNvbWVzIHN0YWJsZVxuICAgIC8vICAgc2VlOiBodHRwczovL2dpdGh1Yi5jb20vcnVzdC1sYW5nL2NhcmdvL2lzc3Vlcy85MzAxXG4gICAgY29uc3QgbGlua2VyRW52ID0gYENBUkdPX1RBUkdFVF8ke3RhcmdldFRvRW52VmFyKFxuICAgICAgdGhpcy50YXJnZXQudHJpcGxlLFxuICAgICl9X0xJTktFUmBcbiAgICBpZiAobGlua2VyICYmICFwcm9jZXNzLmVudltsaW5rZXJFbnZdICYmICF0aGlzLmVudnNbbGlua2VyRW52XSkge1xuICAgICAgdGhpcy5lbnZzW2xpbmtlckVudl0gPSBsaW5rZXJcbiAgICB9XG5cbiAgICBpZiAodGhpcy50YXJnZXQucGxhdGZvcm0gPT09ICdhbmRyb2lkJykge1xuICAgICAgdGhpcy5zZXRBbmRyb2lkRW52KClcbiAgICB9XG5cbiAgICBpZiAodGhpcy50YXJnZXQucGxhdGZvcm0gPT09ICd3YXNpJykge1xuICAgICAgdGhpcy5zZXRXYXNpRW52KClcbiAgICB9XG5cbiAgICBpZiAodGhpcy50YXJnZXQucGxhdGZvcm0gPT09ICdvcGVuaGFybW9ueScpIHtcbiAgICAgIHRoaXMuc2V0T3Blbkhhcm1vbnlFbnYoKVxuICAgIH1cblxuICAgIGRlYnVnKCdTZXQgZW52czogJylcbiAgICBPYmplY3QuZW50cmllcyh0aGlzLmVudnMpLmZvckVhY2goKFtrLCB2XSkgPT4ge1xuICAgICAgZGVidWcoJyAgJWknLCBgJHtrfT0ke3Z9YClcbiAgICB9KVxuXG4gICAgcmV0dXJuIHRoaXNcbiAgfVxuXG4gIHByaXZhdGUgc2V0Rm9yY2VCdWlsZEVudnModHlwZURlZlRtcEZvbGRlcjogc3RyaW5nKSB7XG4gICAgLy8gZHluYW1pY2FsbHkgY2hlY2sgYWxsIG5hcGktcnMgZGVwcyBhbmQgc2V0IGBOQVBJX0ZPUkNFX0JVSUxEX3t1cHBlcmNhc2Uoc25ha2VfY2FzZShuYW1lKSl9ID0gdGltZXN0YW1wYFxuICAgIHRoaXMubWV0YWRhdGEucGFja2FnZXMuZm9yRWFjaCgoY3JhdGUpID0+IHtcbiAgICAgIGlmIChcbiAgICAgICAgY3JhdGUuZGVwZW5kZW5jaWVzLnNvbWUoKGQpID0+IGQubmFtZSA9PT0gJ25hcGktZGVyaXZlJykgJiZcbiAgICAgICAgIWV4aXN0c1N5bmMoam9pbih0eXBlRGVmVG1wRm9sZGVyLCBjcmF0ZS5uYW1lKSlcbiAgICAgICkge1xuICAgICAgICB0aGlzLmVudnNbXG4gICAgICAgICAgYE5BUElfRk9SQ0VfQlVJTERfJHtjcmF0ZS5uYW1lLnJlcGxhY2UoLy0vZywgJ18nKS50b1VwcGVyQ2FzZSgpfWBcbiAgICAgICAgXSA9IERhdGUubm93KCkudG9TdHJpbmcoKVxuICAgICAgfVxuICAgIH0pXG4gIH1cblxuICBwcml2YXRlIHNldEFuZHJvaWRFbnYoKSB7XG4gICAgY29uc3QgeyBBTkRST0lEX05ES19MQVRFU1RfSE9NRSB9ID0gcHJvY2Vzcy5lbnZcbiAgICBpZiAoIUFORFJPSURfTkRLX0xBVEVTVF9IT01FKSB7XG4gICAgICBkZWJ1Zy53YXJuKFxuICAgICAgICBgJHtjb2xvcnMucmVkKFxuICAgICAgICAgICdBTkRST0lEX05ES19MQVRFU1RfSE9NRScsXG4gICAgICAgICl9IGVudmlyb25tZW50IHZhcmlhYmxlIGlzIG1pc3NpbmdgLFxuICAgICAgKVxuICAgIH1cblxuICAgIC8vIHNraXAgY3Jvc3MgY29tcGlsZSBzZXR1cCBpZiBob3N0IGlzIGFuZHJvaWRcbiAgICBpZiAocHJvY2Vzcy5wbGF0Zm9ybSA9PT0gJ2FuZHJvaWQnKSB7XG4gICAgICByZXR1cm5cbiAgICB9XG5cbiAgICBjb25zdCB0YXJnZXRBcmNoID0gdGhpcy50YXJnZXQuYXJjaCA9PT0gJ2FybScgPyAnYXJtdjdhJyA6ICdhYXJjaDY0J1xuICAgIGNvbnN0IHRhcmdldFBsYXRmb3JtID1cbiAgICAgIHRoaXMudGFyZ2V0LmFyY2ggPT09ICdhcm0nID8gJ2FuZHJvaWRlYWJpMjQnIDogJ2FuZHJvaWQyNCdcbiAgICBjb25zdCBob3N0UGxhdGZvcm0gPVxuICAgICAgcHJvY2Vzcy5wbGF0Zm9ybSA9PT0gJ2RhcndpbidcbiAgICAgICAgPyAnZGFyd2luJ1xuICAgICAgICA6IHByb2Nlc3MucGxhdGZvcm0gPT09ICd3aW4zMidcbiAgICAgICAgICA/ICd3aW5kb3dzJ1xuICAgICAgICAgIDogJ2xpbnV4J1xuICAgIE9iamVjdC5hc3NpZ24odGhpcy5lbnZzLCB7XG4gICAgICBDQVJHT19UQVJHRVRfQUFSQ0g2NF9MSU5VWF9BTkRST0lEX0xJTktFUjogYCR7QU5EUk9JRF9OREtfTEFURVNUX0hPTUV9L3Rvb2xjaGFpbnMvbGx2bS9wcmVidWlsdC8ke2hvc3RQbGF0Zm9ybX0teDg2XzY0L2Jpbi8ke3RhcmdldEFyY2h9LWxpbnV4LWFuZHJvaWQyNC1jbGFuZ2AsXG4gICAgICBDQVJHT19UQVJHRVRfQVJNVjdfTElOVVhfQU5EUk9JREVBQklfTElOS0VSOiBgJHtBTkRST0lEX05ES19MQVRFU1RfSE9NRX0vdG9vbGNoYWlucy9sbHZtL3ByZWJ1aWx0LyR7aG9zdFBsYXRmb3JtfS14ODZfNjQvYmluLyR7dGFyZ2V0QXJjaH0tbGludXgtYW5kcm9pZGVhYmkyNC1jbGFuZ2AsXG4gICAgICBUQVJHRVRfQ0M6IGAke0FORFJPSURfTkRLX0xBVEVTVF9IT01FfS90b29sY2hhaW5zL2xsdm0vcHJlYnVpbHQvJHtob3N0UGxhdGZvcm19LXg4Nl82NC9iaW4vJHt0YXJnZXRBcmNofS1saW51eC0ke3RhcmdldFBsYXRmb3JtfS1jbGFuZ2AsXG4gICAgICBUQVJHRVRfQ1hYOiBgJHtBTkRST0lEX05ES19MQVRFU1RfSE9NRX0vdG9vbGNoYWlucy9sbHZtL3ByZWJ1aWx0LyR7aG9zdFBsYXRmb3JtfS14ODZfNjQvYmluLyR7dGFyZ2V0QXJjaH0tbGludXgtJHt0YXJnZXRQbGF0Zm9ybX0tY2xhbmcrK2AsXG4gICAgICBUQVJHRVRfQVI6IGAke0FORFJPSURfTkRLX0xBVEVTVF9IT01FfS90b29sY2hhaW5zL2xsdm0vcHJlYnVpbHQvJHtob3N0UGxhdGZvcm19LXg4Nl82NC9iaW4vbGx2bS1hcmAsXG4gICAgICBUQVJHRVRfUkFOTElCOiBgJHtBTkRST0lEX05ES19MQVRFU1RfSE9NRX0vdG9vbGNoYWlucy9sbHZtL3ByZWJ1aWx0LyR7aG9zdFBsYXRmb3JtfS14ODZfNjQvYmluL2xsdm0tcmFubGliYCxcbiAgICAgIEFORFJPSURfTkRLOiBBTkRST0lEX05ES19MQVRFU1RfSE9NRSxcbiAgICAgIFBBVEg6IGAke0FORFJPSURfTkRLX0xBVEVTVF9IT01FfS90b29sY2hhaW5zL2xsdm0vcHJlYnVpbHQvJHtob3N0UGxhdGZvcm19LXg4Nl82NC9iaW4ke3Byb2Nlc3MucGxhdGZvcm0gPT09ICd3aW4zMicgPyAnOycgOiAnOid9JHtwcm9jZXNzLmVudi5QQVRIfWAsXG4gICAgfSlcbiAgfVxuXG4gIHByaXZhdGUgc2V0V2FzaUVudigpIHtcbiAgICBjb25zdCBlbW5hcGkgPSBqb2luKFxuICAgICAgcmVxdWlyZS5yZXNvbHZlKCdlbW5hcGknKSxcbiAgICAgICcuLicsXG4gICAgICAnbGliJyxcbiAgICAgICd3YXNtMzItd2FzaS10aHJlYWRzJyxcbiAgICApXG4gICAgdGhpcy5lbnZzLkVNTkFQSV9MSU5LX0RJUiA9IGVtbmFwaVxuICAgIGNvbnN0IHsgV0FTSV9TREtfUEFUSCB9ID0gcHJvY2Vzcy5lbnZcblxuICAgIGlmIChXQVNJX1NES19QQVRIICYmIGV4aXN0c1N5bmMoV0FTSV9TREtfUEFUSCkpIHtcbiAgICAgIHRoaXMuZW52cy5DQVJHT19UQVJHRVRfV0FTTTMyX1dBU0lfUFJFVklFVzFfVEhSRUFEU19MSU5LRVIgPSBqb2luKFxuICAgICAgICBXQVNJX1NES19QQVRILFxuICAgICAgICAnYmluJyxcbiAgICAgICAgJ3dhc20tbGQnLFxuICAgICAgKVxuICAgICAgdGhpcy5lbnZzLkNBUkdPX1RBUkdFVF9XQVNNMzJfV0FTSVAxX0xJTktFUiA9IGpvaW4oXG4gICAgICAgIFdBU0lfU0RLX1BBVEgsXG4gICAgICAgICdiaW4nLFxuICAgICAgICAnd2FzbS1sZCcsXG4gICAgICApXG4gICAgICB0aGlzLmVudnMuQ0FSR09fVEFSR0VUX1dBU00zMl9XQVNJUDFfVEhSRUFEU19MSU5LRVIgPSBqb2luKFxuICAgICAgICBXQVNJX1NES19QQVRILFxuICAgICAgICAnYmluJyxcbiAgICAgICAgJ3dhc20tbGQnLFxuICAgICAgKVxuICAgICAgdGhpcy5lbnZzLkNBUkdPX1RBUkdFVF9XQVNNMzJfV0FTSVAyX0xJTktFUiA9IGpvaW4oXG4gICAgICAgIFdBU0lfU0RLX1BBVEgsXG4gICAgICAgICdiaW4nLFxuICAgICAgICAnd2FzbS1sZCcsXG4gICAgICApXG4gICAgICB0aGlzLnNldEVudklmTm90RXhpc3RzKCdUQVJHRVRfQ0MnLCBqb2luKFdBU0lfU0RLX1BBVEgsICdiaW4nLCAnY2xhbmcnKSlcbiAgICAgIHRoaXMuc2V0RW52SWZOb3RFeGlzdHMoXG4gICAgICAgICdUQVJHRVRfQ1hYJyxcbiAgICAgICAgam9pbihXQVNJX1NES19QQVRILCAnYmluJywgJ2NsYW5nKysnKSxcbiAgICAgIClcbiAgICAgIHRoaXMuc2V0RW52SWZOb3RFeGlzdHMoJ1RBUkdFVF9BUicsIGpvaW4oV0FTSV9TREtfUEFUSCwgJ2JpbicsICdhcicpKVxuICAgICAgdGhpcy5zZXRFbnZJZk5vdEV4aXN0cyhcbiAgICAgICAgJ1RBUkdFVF9SQU5MSUInLFxuICAgICAgICBqb2luKFdBU0lfU0RLX1BBVEgsICdiaW4nLCAncmFubGliJyksXG4gICAgICApXG4gICAgICB0aGlzLnNldEVudklmTm90RXhpc3RzKFxuICAgICAgICAnVEFSR0VUX0NGTEFHUycsXG4gICAgICAgIGAtLXRhcmdldD13YXNtMzItd2FzaS10aHJlYWRzIC0tc3lzcm9vdD0ke1dBU0lfU0RLX1BBVEh9L3NoYXJlL3dhc2ktc3lzcm9vdCAtcHRocmVhZCAtbWxsdm0gLXdhc20tZW5hYmxlLXNqbGpgLFxuICAgICAgKVxuICAgICAgdGhpcy5zZXRFbnZJZk5vdEV4aXN0cyhcbiAgICAgICAgJ1RBUkdFVF9DWFhGTEFHUycsXG4gICAgICAgIGAtLXRhcmdldD13YXNtMzItd2FzaS10aHJlYWRzIC0tc3lzcm9vdD0ke1dBU0lfU0RLX1BBVEh9L3NoYXJlL3dhc2ktc3lzcm9vdCAtcHRocmVhZCAtbWxsdm0gLXdhc20tZW5hYmxlLXNqbGpgLFxuICAgICAgKVxuICAgICAgdGhpcy5zZXRFbnZJZk5vdEV4aXN0cyhcbiAgICAgICAgYFRBUkdFVF9MREZMQUdTYCxcbiAgICAgICAgYC1mdXNlLWxkPSR7V0FTSV9TREtfUEFUSH0vYmluL3dhc20tbGQgLS10YXJnZXQ9d2FzbTMyLXdhc2ktdGhyZWFkc2AsXG4gICAgICApXG4gICAgfVxuICB9XG5cbiAgcHJpdmF0ZSBzZXRPcGVuSGFybW9ueUVudigpIHtcbiAgICBjb25zdCB7IE9IT1NfU0RLX1BBVEgsIE9IT1NfU0RLX05BVElWRSB9ID0gcHJvY2Vzcy5lbnZcbiAgICBjb25zdCBuZGtQYXRoID0gT0hPU19TREtfUEFUSCA/IGAke09IT1NfU0RLX1BBVEh9L25hdGl2ZWAgOiBPSE9TX1NES19OQVRJVkVcbiAgICAvLyBAdHMtZXhwZWN0LWVycm9yXG4gICAgaWYgKCFuZGtQYXRoICYmIHByb2Nlc3MucGxhdGZvcm0gIT09ICdvcGVuaGFybW9ueScpIHtcbiAgICAgIGRlYnVnLndhcm4oXG4gICAgICAgIGAke2NvbG9ycy5yZWQoJ09IT1NfU0RLX1BBVEgnKX0gb3IgJHtjb2xvcnMucmVkKCdPSE9TX1NES19OQVRJVkUnKX0gZW52aXJvbm1lbnQgdmFyaWFibGUgaXMgbWlzc2luZ2AsXG4gICAgICApXG4gICAgICByZXR1cm5cbiAgICB9XG4gICAgY29uc3QgbGlua2VyTmFtZSA9IGBDQVJHT19UQVJHRVRfJHt0aGlzLnRhcmdldC50cmlwbGUudG9VcHBlckNhc2UoKS5yZXBsYWNlKC8tL2csICdfJyl9X0xJTktFUmBcbiAgICBjb25zdCByYW5QYXRoID0gYCR7bmRrUGF0aH0vbGx2bS9iaW4vbGx2bS1yYW5saWJgXG4gICAgY29uc3QgYXJQYXRoID0gYCR7bmRrUGF0aH0vbGx2bS9iaW4vbGx2bS1hcmBcbiAgICBjb25zdCBjY1BhdGggPSBgJHtuZGtQYXRofS9sbHZtL2Jpbi8ke3RoaXMudGFyZ2V0LnRyaXBsZX0tY2xhbmdgXG4gICAgY29uc3QgY3h4UGF0aCA9IGAke25ka1BhdGh9L2xsdm0vYmluLyR7dGhpcy50YXJnZXQudHJpcGxlfS1jbGFuZysrYFxuICAgIGNvbnN0IGFzUGF0aCA9IGAke25ka1BhdGh9L2xsdm0vYmluL2xsdm0tYXNgXG4gICAgY29uc3QgbGRQYXRoID0gYCR7bmRrUGF0aH0vbGx2bS9iaW4vbGQubGxkYFxuICAgIGNvbnN0IHN0cmlwUGF0aCA9IGAke25ka1BhdGh9L2xsdm0vYmluL2xsdm0tc3RyaXBgXG4gICAgY29uc3Qgb2JqRHVtcFBhdGggPSBgJHtuZGtQYXRofS9sbHZtL2Jpbi9sbHZtLW9iamR1bXBgXG4gICAgY29uc3Qgb2JqQ29weVBhdGggPSBgJHtuZGtQYXRofS9sbHZtL2Jpbi9sbHZtLW9iamNvcHlgXG4gICAgY29uc3Qgbm1QYXRoID0gYCR7bmRrUGF0aH0vbGx2bS9iaW4vbGx2bS1ubWBcbiAgICBjb25zdCBiaW5QYXRoID0gYCR7bmRrUGF0aH0vbGx2bS9iaW5gXG4gICAgY29uc3QgbGliUGF0aCA9IGAke25ka1BhdGh9L2xsdm0vbGliYFxuXG4gICAgdGhpcy5zZXRFbnZJZk5vdEV4aXN0cygnTElCQ0xBTkdfUEFUSCcsIGxpYlBhdGgpXG4gICAgdGhpcy5zZXRFbnZJZk5vdEV4aXN0cygnREVQX0FUT01JQycsICdjbGFuZ19ydC5idWlsdGlucycpXG4gICAgdGhpcy5zZXRFbnZJZk5vdEV4aXN0cyhsaW5rZXJOYW1lLCBjY1BhdGgpXG4gICAgdGhpcy5zZXRFbnZJZk5vdEV4aXN0cygnVEFSR0VUX0NDJywgY2NQYXRoKVxuICAgIHRoaXMuc2V0RW52SWZOb3RFeGlzdHMoJ1RBUkdFVF9DWFgnLCBjeHhQYXRoKVxuICAgIHRoaXMuc2V0RW52SWZOb3RFeGlzdHMoJ1RBUkdFVF9BUicsIGFyUGF0aClcbiAgICB0aGlzLnNldEVudklmTm90RXhpc3RzKCdUQVJHRVRfUkFOTElCJywgcmFuUGF0aClcbiAgICB0aGlzLnNldEVudklmTm90RXhpc3RzKCdUQVJHRVRfQVMnLCBhc1BhdGgpXG4gICAgdGhpcy5zZXRFbnZJZk5vdEV4aXN0cygnVEFSR0VUX0xEJywgbGRQYXRoKVxuICAgIHRoaXMuc2V0RW52SWZOb3RFeGlzdHMoJ1RBUkdFVF9TVFJJUCcsIHN0cmlwUGF0aClcbiAgICB0aGlzLnNldEVudklmTm90RXhpc3RzKCdUQVJHRVRfT0JKRFVNUCcsIG9iakR1bXBQYXRoKVxuICAgIHRoaXMuc2V0RW52SWZOb3RFeGlzdHMoJ1RBUkdFVF9PQkpDT1BZJywgb2JqQ29weVBhdGgpXG4gICAgdGhpcy5zZXRFbnZJZk5vdEV4aXN0cygnVEFSR0VUX05NJywgbm1QYXRoKVxuICAgIHRoaXMuZW52cy5QQVRIID0gYCR7YmluUGF0aH0ke3Byb2Nlc3MucGxhdGZvcm0gPT09ICd3aW4zMicgPyAnOycgOiAnOid9JHtwcm9jZXNzLmVudi5QQVRIfWBcbiAgfVxuXG4gIHByaXZhdGUgc2V0RmVhdHVyZXMoKSB7XG4gICAgY29uc3QgYXJncyA9IFtdXG4gICAgaWYgKHRoaXMub3B0aW9ucy5hbGxGZWF0dXJlcyAmJiB0aGlzLm9wdGlvbnMubm9EZWZhdWx0RmVhdHVyZXMpIHtcbiAgICAgIHRocm93IG5ldyBFcnJvcihcbiAgICAgICAgJ0Nhbm5vdCBzcGVjaWZ5IC0tYWxsLWZlYXR1cmVzIGFuZCAtLW5vLWRlZmF1bHQtZmVhdHVyZXMgdG9nZXRoZXInLFxuICAgICAgKVxuICAgIH1cbiAgICBpZiAodGhpcy5vcHRpb25zLmFsbEZlYXR1cmVzKSB7XG4gICAgICBhcmdzLnB1c2goJy0tYWxsLWZlYXR1cmVzJylcbiAgICB9IGVsc2UgaWYgKHRoaXMub3B0aW9ucy5ub0RlZmF1bHRGZWF0dXJlcykge1xuICAgICAgYXJncy5wdXNoKCctLW5vLWRlZmF1bHQtZmVhdHVyZXMnKVxuICAgIH1cbiAgICBpZiAodGhpcy5vcHRpb25zLmZlYXR1cmVzKSB7XG4gICAgICBhcmdzLnB1c2goJy0tZmVhdHVyZXMnLCAuLi50aGlzLm9wdGlvbnMuZmVhdHVyZXMpXG4gICAgfVxuXG4gICAgZGVidWcoJ1NldCBmZWF0dXJlcyBmbGFnczogJylcbiAgICBkZWJ1ZygnICAlTycsIGFyZ3MpXG4gICAgdGhpcy5hcmdzLnB1c2goLi4uYXJncylcblxuICAgIHJldHVybiB0aGlzXG4gIH1cblxuICBwcml2YXRlIHNldEJ5cGFzc0FyZ3MoKSB7XG4gICAgaWYgKHRoaXMub3B0aW9ucy5yZWxlYXNlKSB7XG4gICAgICB0aGlzLmFyZ3MucHVzaCgnLS1yZWxlYXNlJylcbiAgICB9XG5cbiAgICBpZiAodGhpcy5vcHRpb25zLnZlcmJvc2UpIHtcbiAgICAgIHRoaXMuYXJncy5wdXNoKCctLXZlcmJvc2UnKVxuICAgIH1cblxuICAgIGlmICh0aGlzLm9wdGlvbnMudGFyZ2V0RGlyKSB7XG4gICAgICB0aGlzLmFyZ3MucHVzaCgnLS10YXJnZXQtZGlyJywgdGhpcy5vcHRpb25zLnRhcmdldERpcilcbiAgICB9XG5cbiAgICBpZiAodGhpcy5vcHRpb25zLnByb2ZpbGUpIHtcbiAgICAgIHRoaXMuYXJncy5wdXNoKCctLXByb2ZpbGUnLCB0aGlzLm9wdGlvbnMucHJvZmlsZSlcbiAgICB9XG5cbiAgICBpZiAodGhpcy5vcHRpb25zLm1hbmlmZXN0UGF0aCkge1xuICAgICAgdGhpcy5hcmdzLnB1c2goJy0tbWFuaWZlc3QtcGF0aCcsIHRoaXMub3B0aW9ucy5tYW5pZmVzdFBhdGgpXG4gICAgfVxuXG4gICAgaWYgKHRoaXMub3B0aW9ucy5jYXJnb09wdGlvbnM/Lmxlbmd0aCkge1xuICAgICAgdGhpcy5hcmdzLnB1c2goLi4udGhpcy5vcHRpb25zLmNhcmdvT3B0aW9ucylcbiAgICB9XG5cbiAgICByZXR1cm4gdGhpc1xuICB9XG5cbiAgcHJpdmF0ZSBnZW5lcmF0ZUludGVybWVkaWF0ZVR5cGVEZWZGb2xkZXIoKSB7XG4gICAgbGV0IGZvbGRlciA9IGpvaW4oXG4gICAgICB0aGlzLnRhcmdldERpcixcbiAgICAgICduYXBpLXJzJyxcbiAgICAgIGAke3RoaXMuY3JhdGUubmFtZX0tJHtjcmVhdGVIYXNoKCdzaGEyNTYnKVxuICAgICAgICAudXBkYXRlKHRoaXMuY3JhdGUubWFuaWZlc3RfcGF0aClcbiAgICAgICAgLnVwZGF0ZShDTElfVkVSU0lPTilcbiAgICAgICAgLmRpZ2VzdCgnaGV4JylcbiAgICAgICAgLnN1YnN0cmluZygwLCA4KX1gLFxuICAgIClcblxuICAgIGlmICghdGhpcy5vcHRpb25zLmR0c0NhY2hlKSB7XG4gICAgICBybVN5bmMoZm9sZGVyLCB7IHJlY3Vyc2l2ZTogdHJ1ZSwgZm9yY2U6IHRydWUgfSlcbiAgICAgIGZvbGRlciArPSBgXyR7RGF0ZS5ub3coKX1gXG4gICAgfVxuXG4gICAgbWtkaXJBc3luYyhmb2xkZXIsIHsgcmVjdXJzaXZlOiB0cnVlIH0pXG5cbiAgICByZXR1cm4gZm9sZGVyXG4gIH1cblxuICBwcml2YXRlIGFzeW5jIHBvc3RCdWlsZCgpIHtcbiAgICB0cnkge1xuICAgICAgZGVidWcoYFRyeSB0byBjcmVhdGUgb3V0cHV0IGRpcmVjdG9yeTpgKVxuICAgICAgZGVidWcoJyAgJWknLCB0aGlzLm91dHB1dERpcilcbiAgICAgIGF3YWl0IG1rZGlyQXN5bmModGhpcy5vdXRwdXREaXIsIHsgcmVjdXJzaXZlOiB0cnVlIH0pXG4gICAgICBkZWJ1ZyhgT3V0cHV0IGRpcmVjdG9yeSBjcmVhdGVkYClcbiAgICB9IGNhdGNoIChlKSB7XG4gICAgICB0aHJvdyBuZXcgRXJyb3IoYEZhaWxlZCB0byBjcmVhdGUgb3V0cHV0IGRpcmVjdG9yeSAke3RoaXMub3V0cHV0RGlyfWAsIHtcbiAgICAgICAgY2F1c2U6IGUsXG4gICAgICB9KVxuICAgIH1cblxuICAgIGNvbnN0IHdhc21CaW5hcnlOYW1lID0gYXdhaXQgdGhpcy5jb3B5QXJ0aWZhY3QoKVxuXG4gICAgLy8gb25seSBmb3IgY2R5bGliXG4gICAgaWYgKHRoaXMuY2R5TGliTmFtZSkge1xuICAgICAgY29uc3QgaWRlbnRzID0gYXdhaXQgdGhpcy5nZW5lcmF0ZVR5cGVEZWYoKVxuICAgICAgY29uc3QganNPdXRwdXQgPSBhd2FpdCB0aGlzLndyaXRlSnNCaW5kaW5nKGlkZW50cylcbiAgICAgIGNvbnN0IHdhc21CaW5kaW5nc091dHB1dCA9IGF3YWl0IHRoaXMud3JpdGVXYXNpQmluZGluZyhcbiAgICAgICAgd2FzbUJpbmFyeU5hbWUsXG4gICAgICAgIGlkZW50cyxcbiAgICAgIClcbiAgICAgIGlmIChqc091dHB1dCkge1xuICAgICAgICB0aGlzLm91dHB1dHMucHVzaChqc091dHB1dClcbiAgICAgIH1cbiAgICAgIGlmICh3YXNtQmluZGluZ3NPdXRwdXQpIHtcbiAgICAgICAgdGhpcy5vdXRwdXRzLnB1c2goLi4ud2FzbUJpbmRpbmdzT3V0cHV0KVxuICAgICAgfVxuICAgIH1cblxuICAgIHJldHVybiB0aGlzLm91dHB1dHNcbiAgfVxuXG4gIHByaXZhdGUgYXN5bmMgY29weUFydGlmYWN0KCkge1xuICAgIGNvbnN0IFtzcmNOYW1lLCBkZXN0TmFtZSwgd2FzbUJpbmFyeU5hbWVdID0gdGhpcy5nZXRBcnRpZmFjdE5hbWVzKClcbiAgICBpZiAoIXNyY05hbWUgfHwgIWRlc3ROYW1lKSB7XG4gICAgICByZXR1cm5cbiAgICB9XG5cbiAgICBjb25zdCBwcm9maWxlID1cbiAgICAgIHRoaXMub3B0aW9ucy5wcm9maWxlID8/ICh0aGlzLm9wdGlvbnMucmVsZWFzZSA/ICdyZWxlYXNlJyA6ICdkZWJ1ZycpXG4gICAgY29uc3Qgc3JjID0gam9pbih0aGlzLnRhcmdldERpciwgdGhpcy50YXJnZXQudHJpcGxlLCBwcm9maWxlLCBzcmNOYW1lKVxuICAgIGRlYnVnKGBDb3B5IGFydGlmYWN0IGZyb206IFske3NyY31dYClcbiAgICBjb25zdCBkZXN0ID0gam9pbih0aGlzLm91dHB1dERpciwgZGVzdE5hbWUpXG4gICAgY29uc3QgaXNXYXNtID0gZGVzdC5lbmRzV2l0aCgnLndhc20nKVxuXG4gICAgdHJ5IHtcbiAgICAgIGlmIChhd2FpdCBmaWxlRXhpc3RzKGRlc3QpKSB7XG4gICAgICAgIGRlYnVnKCdPbGQgYXJ0aWZhY3QgZm91bmQsIHJlbW92ZSBpdCBmaXJzdCcpXG4gICAgICAgIGF3YWl0IHVubGlua0FzeW5jKGRlc3QpXG4gICAgICB9XG4gICAgICBkZWJ1ZygnQ29weSBhcnRpZmFjdCB0bzonKVxuICAgICAgZGVidWcoJyAgJWknLCBkZXN0KVxuICAgICAgaWYgKGlzV2FzbSkge1xuICAgICAgICBjb25zdCB7IE1vZHVsZUNvbmZpZyB9ID0gYXdhaXQgaW1wb3J0KCdAbmFwaS1ycy93YXNtLXRvb2xzJylcbiAgICAgICAgZGVidWcoJ0dlbmVyYXRlIGRlYnVnIHdhc20gbW9kdWxlJylcbiAgICAgICAgdHJ5IHtcbiAgICAgICAgICBjb25zdCBkZWJ1Z1dhc21Nb2R1bGUgPSBuZXcgTW9kdWxlQ29uZmlnKClcbiAgICAgICAgICAgIC5nZW5lcmF0ZUR3YXJmKHRydWUpXG4gICAgICAgICAgICAuZ2VuZXJhdGVOYW1lU2VjdGlvbih0cnVlKVxuICAgICAgICAgICAgLmdlbmVyYXRlUHJvZHVjZXJzU2VjdGlvbih0cnVlKVxuICAgICAgICAgICAgLnByZXNlcnZlQ29kZVRyYW5zZm9ybSh0cnVlKVxuICAgICAgICAgICAgLnN0cmljdFZhbGlkYXRlKGZhbHNlKVxuICAgICAgICAgICAgLnBhcnNlKGF3YWl0IHJlYWRGaWxlQXN5bmMoc3JjKSlcbiAgICAgICAgICBjb25zdCBkZWJ1Z1dhc21CaW5hcnkgPSBkZWJ1Z1dhc21Nb2R1bGUuZW1pdFdhc20odHJ1ZSlcbiAgICAgICAgICBhd2FpdCB3cml0ZUZpbGVBc3luYyhcbiAgICAgICAgICAgIGRlc3QucmVwbGFjZSgvXFwud2FzbSQvLCAnLmRlYnVnLndhc20nKSxcbiAgICAgICAgICAgIGRlYnVnV2FzbUJpbmFyeSxcbiAgICAgICAgICApXG4gICAgICAgICAgZGVidWcoJ0dlbmVyYXRlIHJlbGVhc2Ugd2FzbSBtb2R1bGUnKVxuICAgICAgICAgIGNvbnN0IHJlbGVhc2VXYXNtTW9kdWxlID0gbmV3IE1vZHVsZUNvbmZpZygpXG4gICAgICAgICAgICAuZ2VuZXJhdGVEd2FyZihmYWxzZSlcbiAgICAgICAgICAgIC5nZW5lcmF0ZU5hbWVTZWN0aW9uKGZhbHNlKVxuICAgICAgICAgICAgLmdlbmVyYXRlUHJvZHVjZXJzU2VjdGlvbihmYWxzZSlcbiAgICAgICAgICAgIC5wcmVzZXJ2ZUNvZGVUcmFuc2Zvcm0oZmFsc2UpXG4gICAgICAgICAgICAuc3RyaWN0VmFsaWRhdGUoZmFsc2UpXG4gICAgICAgICAgICAub25seVN0YWJsZUZlYXR1cmVzKGZhbHNlKVxuICAgICAgICAgICAgLnBhcnNlKGRlYnVnV2FzbUJpbmFyeSlcbiAgICAgICAgICBjb25zdCByZWxlYXNlV2FzbUJpbmFyeSA9IHJlbGVhc2VXYXNtTW9kdWxlLmVtaXRXYXNtKGZhbHNlKVxuICAgICAgICAgIGF3YWl0IHdyaXRlRmlsZUFzeW5jKGRlc3QsIHJlbGVhc2VXYXNtQmluYXJ5KVxuICAgICAgICB9IGNhdGNoIChlKSB7XG4gICAgICAgICAgZGVidWcud2FybihcbiAgICAgICAgICAgIGBGYWlsZWQgdG8gZ2VuZXJhdGUgZGVidWcgd2FzbSBtb2R1bGU6ICR7KGUgYXMgYW55KS5tZXNzYWdlID8/IGV9YCxcbiAgICAgICAgICApXG4gICAgICAgICAgYXdhaXQgY29weUZpbGVBc3luYyhzcmMsIGRlc3QpXG4gICAgICAgIH1cbiAgICAgIH0gZWxzZSB7XG4gICAgICAgIGF3YWl0IGNvcHlGaWxlQXN5bmMoc3JjLCBkZXN0KVxuICAgICAgfVxuICAgICAgdGhpcy5vdXRwdXRzLnB1c2goe1xuICAgICAgICBraW5kOiBkZXN0LmVuZHNXaXRoKCcubm9kZScpID8gJ25vZGUnIDogaXNXYXNtID8gJ3dhc20nIDogJ2V4ZScsXG4gICAgICAgIHBhdGg6IGRlc3QsXG4gICAgICB9KVxuICAgICAgcmV0dXJuIHdhc21CaW5hcnlOYW1lID8gam9pbih0aGlzLm91dHB1dERpciwgd2FzbUJpbmFyeU5hbWUpIDogbnVsbFxuICAgIH0gY2F0Y2ggKGUpIHtcbiAgICAgIHRocm93IG5ldyBFcnJvcignRmFpbGVkIHRvIGNvcHkgYXJ0aWZhY3QnLCB7IGNhdXNlOiBlIH0pXG4gICAgfVxuICB9XG5cbiAgcHJpdmF0ZSBnZXRBcnRpZmFjdE5hbWVzKCkge1xuICAgIGlmICh0aGlzLmNkeUxpYk5hbWUpIHtcbiAgICAgIGNvbnN0IGNkeUxpYiA9IHRoaXMuY2R5TGliTmFtZS5yZXBsYWNlKC8tL2csICdfJylcbiAgICAgIGNvbnN0IHdhc2lUYXJnZXQgPSB0aGlzLmNvbmZpZy50YXJnZXRzLmZpbmQoKHQpID0+IHQucGxhdGZvcm0gPT09ICd3YXNpJylcblxuICAgICAgY29uc3Qgc3JjTmFtZSA9XG4gICAgICAgIHRoaXMudGFyZ2V0LnBsYXRmb3JtID09PSAnZGFyd2luJ1xuICAgICAgICAgID8gYGxpYiR7Y2R5TGlifS5keWxpYmBcbiAgICAgICAgICA6IHRoaXMudGFyZ2V0LnBsYXRmb3JtID09PSAnd2luMzInXG4gICAgICAgICAgICA/IGAke2NkeUxpYn0uZGxsYFxuICAgICAgICAgICAgOiB0aGlzLnRhcmdldC5wbGF0Zm9ybSA9PT0gJ3dhc2knIHx8IHRoaXMudGFyZ2V0LnBsYXRmb3JtID09PSAnd2FzbSdcbiAgICAgICAgICAgICAgPyBgJHtjZHlMaWJ9Lndhc21gXG4gICAgICAgICAgICAgIDogYGxpYiR7Y2R5TGlifS5zb2BcblxuICAgICAgbGV0IGRlc3ROYW1lID0gdGhpcy5jb25maWcuYmluYXJ5TmFtZVxuICAgICAgLy8gYWRkIHBsYXRmb3JtIHN1ZmZpeCB0byBiaW5hcnkgbmFtZVxuICAgICAgLy8gaW5kZXhbLmxpbnV4LXg2NC1nbnVdLm5vZGVcbiAgICAgIC8vICAgICAgIF5eXl5eXl5eXl5eXl5eXG4gICAgICBpZiAodGhpcy5vcHRpb25zLnBsYXRmb3JtKSB7XG4gICAgICAgIGRlc3ROYW1lICs9IGAuJHt0aGlzLnRhcmdldC5wbGF0Zm9ybUFyY2hBQkl9YFxuICAgICAgfVxuICAgICAgaWYgKHNyY05hbWUuZW5kc1dpdGgoJy53YXNtJykpIHtcbiAgICAgICAgZGVzdE5hbWUgKz0gJy53YXNtJ1xuICAgICAgfSBlbHNlIHtcbiAgICAgICAgZGVzdE5hbWUgKz0gJy5ub2RlJ1xuICAgICAgfVxuXG4gICAgICByZXR1cm4gW1xuICAgICAgICBzcmNOYW1lLFxuICAgICAgICBkZXN0TmFtZSxcbiAgICAgICAgd2FzaVRhcmdldFxuICAgICAgICAgID8gYCR7dGhpcy5jb25maWcuYmluYXJ5TmFtZX0uJHt3YXNpVGFyZ2V0LnBsYXRmb3JtQXJjaEFCSX0ud2FzbWBcbiAgICAgICAgICA6IG51bGwsXG4gICAgICBdXG4gICAgfSBlbHNlIGlmICh0aGlzLmJpbk5hbWUpIHtcbiAgICAgIGNvbnN0IHNyY05hbWUgPVxuICAgICAgICB0aGlzLnRhcmdldC5wbGF0Zm9ybSA9PT0gJ3dpbjMyJyA/IGAke3RoaXMuYmluTmFtZX0uZXhlYCA6IHRoaXMuYmluTmFtZVxuXG4gICAgICByZXR1cm4gW3NyY05hbWUsIHNyY05hbWVdXG4gICAgfVxuXG4gICAgcmV0dXJuIFtdXG4gIH1cblxuICBwcml2YXRlIGFzeW5jIGdlbmVyYXRlVHlwZURlZigpIHtcbiAgICBjb25zdCB0eXBlRGVmRGlyID0gdGhpcy5lbnZzLk5BUElfVFlQRV9ERUZfVE1QX0ZPTERFUlxuICAgIGlmICghdGhpcy5lbmFibGVUeXBlRGVmKSB7XG4gICAgICByZXR1cm4gW11cbiAgICB9XG5cbiAgICBjb25zdCB7IGV4cG9ydHMsIGR0cyB9ID0gYXdhaXQgZ2VuZXJhdGVUeXBlRGVmKHtcbiAgICAgIHR5cGVEZWZEaXIsXG4gICAgICBub0R0c0hlYWRlcjogdGhpcy5vcHRpb25zLm5vRHRzSGVhZGVyLFxuICAgICAgZHRzSGVhZGVyOiB0aGlzLm9wdGlvbnMuZHRzSGVhZGVyLFxuICAgICAgY29uZmlnRHRzSGVhZGVyOiB0aGlzLmNvbmZpZy5kdHNIZWFkZXIsXG4gICAgICBjb25maWdEdHNIZWFkZXJGaWxlOiB0aGlzLmNvbmZpZy5kdHNIZWFkZXJGaWxlLFxuICAgICAgY29uc3RFbnVtOiB0aGlzLm9wdGlvbnMuY29uc3RFbnVtID8/IHRoaXMuY29uZmlnLmNvbnN0RW51bSxcbiAgICAgIGN3ZDogdGhpcy5vcHRpb25zLmN3ZCxcbiAgICB9KVxuXG4gICAgY29uc3QgZGVzdCA9IGpvaW4odGhpcy5vdXRwdXREaXIsIHRoaXMub3B0aW9ucy5kdHMgPz8gJ2luZGV4LmQudHMnKVxuXG4gICAgdHJ5IHtcbiAgICAgIGRlYnVnKCdXcml0aW5nIHR5cGUgZGVmIHRvOicpXG4gICAgICBkZWJ1ZygnICAlaScsIGRlc3QpXG4gICAgICBhd2FpdCB3cml0ZUZpbGVBc3luYyhkZXN0LCBkdHMsICd1dGYtOCcpXG4gICAgfSBjYXRjaCAoZSkge1xuICAgICAgZGVidWcuZXJyb3IoJ0ZhaWxlZCB0byB3cml0ZSB0eXBlIGRlZiBmaWxlJylcbiAgICAgIGRlYnVnLmVycm9yKGUgYXMgRXJyb3IpXG4gICAgfVxuXG4gICAgaWYgKGV4cG9ydHMubGVuZ3RoID4gMCkge1xuICAgICAgY29uc3QgZGVzdCA9IGpvaW4odGhpcy5vdXRwdXREaXIsIHRoaXMub3B0aW9ucy5kdHMgPz8gJ2luZGV4LmQudHMnKVxuICAgICAgdGhpcy5vdXRwdXRzLnB1c2goeyBraW5kOiAnZHRzJywgcGF0aDogZGVzdCB9KVxuICAgIH1cblxuICAgIHJldHVybiBleHBvcnRzXG4gIH1cblxuICBwcml2YXRlIGFzeW5jIHdyaXRlSnNCaW5kaW5nKGlkZW50czogc3RyaW5nW10pIHtcbiAgICByZXR1cm4gd3JpdGVKc0JpbmRpbmcoe1xuICAgICAgcGxhdGZvcm06IHRoaXMub3B0aW9ucy5wbGF0Zm9ybSxcbiAgICAgIG5vSnNCaW5kaW5nOiB0aGlzLm9wdGlvbnMubm9Kc0JpbmRpbmcsXG4gICAgICBpZGVudHMsXG4gICAgICBqc0JpbmRpbmc6IHRoaXMub3B0aW9ucy5qc0JpbmRpbmcsXG4gICAgICBlc206IHRoaXMub3B0aW9ucy5lc20sXG4gICAgICBiaW5hcnlOYW1lOiB0aGlzLmNvbmZpZy5iaW5hcnlOYW1lLFxuICAgICAgcGFja2FnZU5hbWU6IHRoaXMub3B0aW9ucy5qc1BhY2thZ2VOYW1lID8/IHRoaXMuY29uZmlnLnBhY2thZ2VOYW1lLFxuICAgICAgdmVyc2lvbjogcHJvY2Vzcy5lbnYubnBtX25ld192ZXJzaW9uID8/IHRoaXMuY29uZmlnLnBhY2thZ2VKc29uLnZlcnNpb24sXG4gICAgICBvdXRwdXREaXI6IHRoaXMub3V0cHV0RGlyLFxuICAgIH0pXG4gIH1cblxuICBwcml2YXRlIGFzeW5jIHdyaXRlV2FzaUJpbmRpbmcoXG4gICAgZGlzdEZpbGVOYW1lOiBzdHJpbmcgfCB1bmRlZmluZWQgfCBudWxsLFxuICAgIGlkZW50czogc3RyaW5nW10sXG4gICkge1xuICAgIGlmIChkaXN0RmlsZU5hbWUpIHtcbiAgICAgIGNvbnN0IHsgbmFtZSwgZGlyIH0gPSBwYXJzZShkaXN0RmlsZU5hbWUpXG4gICAgICBjb25zdCBiaW5kaW5nUGF0aCA9IGpvaW4oZGlyLCBgJHt0aGlzLmNvbmZpZy5iaW5hcnlOYW1lfS53YXNpLmNqc2ApXG4gICAgICBjb25zdCBicm93c2VyQmluZGluZ1BhdGggPSBqb2luKFxuICAgICAgICBkaXIsXG4gICAgICAgIGAke3RoaXMuY29uZmlnLmJpbmFyeU5hbWV9Lndhc2ktYnJvd3Nlci5qc2AsXG4gICAgICApXG4gICAgICBjb25zdCB3b3JrZXJQYXRoID0gam9pbihkaXIsICd3YXNpLXdvcmtlci5tanMnKVxuICAgICAgY29uc3QgYnJvd3NlcldvcmtlclBhdGggPSBqb2luKGRpciwgJ3dhc2ktd29ya2VyLWJyb3dzZXIubWpzJylcbiAgICAgIGNvbnN0IGJyb3dzZXJFbnRyeVBhdGggPSBqb2luKGRpciwgJ2Jyb3dzZXIuanMnKVxuICAgICAgY29uc3QgZXhwb3J0c0NvZGUgPVxuICAgICAgICBgbW9kdWxlLmV4cG9ydHMgPSBfX25hcGlNb2R1bGUuZXhwb3J0c1xcbmAgK1xuICAgICAgICBpZGVudHNcbiAgICAgICAgICAubWFwKFxuICAgICAgICAgICAgKGlkZW50KSA9PlxuICAgICAgICAgICAgICBgbW9kdWxlLmV4cG9ydHMuJHtpZGVudH0gPSBfX25hcGlNb2R1bGUuZXhwb3J0cy4ke2lkZW50fWAsXG4gICAgICAgICAgKVxuICAgICAgICAgIC5qb2luKCdcXG4nKVxuICAgICAgYXdhaXQgd3JpdGVGaWxlQXN5bmMoXG4gICAgICAgIGJpbmRpbmdQYXRoLFxuICAgICAgICBjcmVhdGVXYXNpQmluZGluZyhcbiAgICAgICAgICBuYW1lLFxuICAgICAgICAgIHRoaXMuY29uZmlnLnBhY2thZ2VOYW1lLFxuICAgICAgICAgIHRoaXMuY29uZmlnLndhc20/LmluaXRpYWxNZW1vcnksXG4gICAgICAgICAgdGhpcy5jb25maWcud2FzbT8ubWF4aW11bU1lbW9yeSxcbiAgICAgICAgKSArXG4gICAgICAgICAgZXhwb3J0c0NvZGUgK1xuICAgICAgICAgICdcXG4nLFxuICAgICAgICAndXRmOCcsXG4gICAgICApXG4gICAgICBhd2FpdCB3cml0ZUZpbGVBc3luYyhcbiAgICAgICAgYnJvd3NlckJpbmRpbmdQYXRoLFxuICAgICAgICBjcmVhdGVXYXNpQnJvd3NlckJpbmRpbmcoXG4gICAgICAgICAgbmFtZSxcbiAgICAgICAgICB0aGlzLmNvbmZpZy53YXNtPy5pbml0aWFsTWVtb3J5LFxuICAgICAgICAgIHRoaXMuY29uZmlnLndhc20/Lm1heGltdW1NZW1vcnksXG4gICAgICAgICAgdGhpcy5jb25maWcud2FzbT8uYnJvd3Nlcj8uZnMsXG4gICAgICAgICAgdGhpcy5jb25maWcud2FzbT8uYnJvd3Nlcj8uYXN5bmNJbml0LFxuICAgICAgICAgIHRoaXMuY29uZmlnLndhc20/LmJyb3dzZXI/LmJ1ZmZlcixcbiAgICAgICAgKSArXG4gICAgICAgICAgYGV4cG9ydCBkZWZhdWx0IF9fbmFwaU1vZHVsZS5leHBvcnRzXFxuYCArXG4gICAgICAgICAgaWRlbnRzXG4gICAgICAgICAgICAubWFwKFxuICAgICAgICAgICAgICAoaWRlbnQpID0+XG4gICAgICAgICAgICAgICAgYGV4cG9ydCBjb25zdCAke2lkZW50fSA9IF9fbmFwaU1vZHVsZS5leHBvcnRzLiR7aWRlbnR9YCxcbiAgICAgICAgICAgIClcbiAgICAgICAgICAgIC5qb2luKCdcXG4nKSArXG4gICAgICAgICAgJ1xcbicsXG4gICAgICAgICd1dGY4JyxcbiAgICAgIClcbiAgICAgIGF3YWl0IHdyaXRlRmlsZUFzeW5jKHdvcmtlclBhdGgsIFdBU0lfV09SS0VSX1RFTVBMQVRFLCAndXRmOCcpXG4gICAgICBhd2FpdCB3cml0ZUZpbGVBc3luYyhcbiAgICAgICAgYnJvd3NlcldvcmtlclBhdGgsXG4gICAgICAgIGNyZWF0ZVdhc2lCcm93c2VyV29ya2VyQmluZGluZyh0aGlzLmNvbmZpZy53YXNtPy5icm93c2VyPy5mcyA/PyBmYWxzZSksXG4gICAgICAgICd1dGY4JyxcbiAgICAgIClcbiAgICAgIGF3YWl0IHdyaXRlRmlsZUFzeW5jKFxuICAgICAgICBicm93c2VyRW50cnlQYXRoLFxuICAgICAgICBgZXhwb3J0ICogZnJvbSAnJHt0aGlzLmNvbmZpZy5wYWNrYWdlTmFtZX0td2FzbTMyLXdhc2knXFxuYCxcbiAgICAgIClcbiAgICAgIHJldHVybiBbXG4gICAgICAgIHsga2luZDogJ2pzJywgcGF0aDogYmluZGluZ1BhdGggfSxcbiAgICAgICAgeyBraW5kOiAnanMnLCBwYXRoOiBicm93c2VyQmluZGluZ1BhdGggfSxcbiAgICAgICAgeyBraW5kOiAnanMnLCBwYXRoOiB3b3JrZXJQYXRoIH0sXG4gICAgICAgIHsga2luZDogJ2pzJywgcGF0aDogYnJvd3NlcldvcmtlclBhdGggfSxcbiAgICAgICAgeyBraW5kOiAnanMnLCBwYXRoOiBicm93c2VyRW50cnlQYXRoIH0sXG4gICAgICBdIHNhdGlzZmllcyBPdXRwdXRbXVxuICAgIH1cbiAgICByZXR1cm4gW11cbiAgfVxuXG4gIHByaXZhdGUgc2V0RW52SWZOb3RFeGlzdHMoZW52OiBzdHJpbmcsIHZhbHVlOiBzdHJpbmcpIHtcbiAgICBpZiAoIXByb2Nlc3MuZW52W2Vudl0pIHtcbiAgICAgIHRoaXMuZW52c1tlbnZdID0gdmFsdWVcbiAgICB9XG4gIH1cbn1cblxuZXhwb3J0IGludGVyZmFjZSBXcml0ZUpzQmluZGluZ09wdGlvbnMge1xuICBwbGF0Zm9ybT86IGJvb2xlYW5cbiAgbm9Kc0JpbmRpbmc/OiBib29sZWFuXG4gIGlkZW50czogc3RyaW5nW11cbiAganNCaW5kaW5nPzogc3RyaW5nXG4gIGVzbT86IGJvb2xlYW5cbiAgYmluYXJ5TmFtZTogc3RyaW5nXG4gIHBhY2thZ2VOYW1lOiBzdHJpbmdcbiAgdmVyc2lvbjogc3RyaW5nXG4gIG91dHB1dERpcjogc3RyaW5nXG59XG5cbmV4cG9ydCBhc3luYyBmdW5jdGlvbiB3cml0ZUpzQmluZGluZyhcbiAgb3B0aW9uczogV3JpdGVKc0JpbmRpbmdPcHRpb25zLFxuKTogUHJvbWlzZTxPdXRwdXQgfCB1bmRlZmluZWQ+IHtcbiAgaWYgKFxuICAgICFvcHRpb25zLnBsYXRmb3JtIHx8XG4gICAgLy8gZXNsaW50LWRpc2FibGUtbmV4dC1saW5lIEB0eXBlc2NyaXB0LWVzbGludC9wcmVmZXItbnVsbGlzaC1jb2FsZXNjaW5nXG4gICAgb3B0aW9ucy5ub0pzQmluZGluZyB8fFxuICAgIG9wdGlvbnMuaWRlbnRzLmxlbmd0aCA9PT0gMFxuICApIHtcbiAgICByZXR1cm5cbiAgfVxuXG4gIGNvbnN0IG5hbWUgPSBvcHRpb25zLmpzQmluZGluZyA/PyAnaW5kZXguanMnXG5cbiAgY29uc3QgY3JlYXRlQmluZGluZyA9IG9wdGlvbnMuZXNtID8gY3JlYXRlRXNtQmluZGluZyA6IGNyZWF0ZUNqc0JpbmRpbmdcbiAgY29uc3QgYmluZGluZyA9IGNyZWF0ZUJpbmRpbmcoXG4gICAgb3B0aW9ucy5iaW5hcnlOYW1lLFxuICAgIG9wdGlvbnMucGFja2FnZU5hbWUsXG4gICAgb3B0aW9ucy5pZGVudHMsXG4gICAgLy8gaW4gbnBtIHByZXZlcnNpb24gaG9va1xuICAgIG9wdGlvbnMudmVyc2lvbixcbiAgKVxuXG4gIHRyeSB7XG4gICAgY29uc3QgZGVzdCA9IGpvaW4ob3B0aW9ucy5vdXRwdXREaXIsIG5hbWUpXG4gICAgZGVidWcoJ1dyaXRpbmcganMgYmluZGluZyB0bzonKVxuICAgIGRlYnVnKCcgICVpJywgZGVzdClcbiAgICBhd2FpdCB3cml0ZUZpbGVBc3luYyhkZXN0LCBiaW5kaW5nLCAndXRmLTgnKVxuICAgIHJldHVybiB7IGtpbmQ6ICdqcycsIHBhdGg6IGRlc3QgfSBzYXRpc2ZpZXMgT3V0cHV0XG4gIH0gY2F0Y2ggKGUpIHtcbiAgICB0aHJvdyBuZXcgRXJyb3IoJ0ZhaWxlZCB0byB3cml0ZSBqcyBiaW5kaW5nIGZpbGUnLCB7IGNhdXNlOiBlIH0pXG4gIH1cbn1cblxuZXhwb3J0IGludGVyZmFjZSBHZW5lcmF0ZVR5cGVEZWZPcHRpb25zIHtcbiAgdHlwZURlZkRpcjogc3RyaW5nXG4gIG5vRHRzSGVhZGVyPzogYm9vbGVhblxuICBkdHNIZWFkZXI/OiBzdHJpbmdcbiAgZHRzSGVhZGVyRmlsZT86IHN0cmluZ1xuICBjb25maWdEdHNIZWFkZXI/OiBzdHJpbmdcbiAgY29uZmlnRHRzSGVhZGVyRmlsZT86IHN0cmluZ1xuICBjb25zdEVudW0/OiBib29sZWFuXG4gIGN3ZDogc3RyaW5nXG59XG5cbmV4cG9ydCBhc3luYyBmdW5jdGlvbiBnZW5lcmF0ZVR5cGVEZWYoXG4gIG9wdGlvbnM6IEdlbmVyYXRlVHlwZURlZk9wdGlvbnMsXG4pOiBQcm9taXNlPHsgZXhwb3J0czogc3RyaW5nW107IGR0czogc3RyaW5nIH0+IHtcbiAgaWYgKCEoYXdhaXQgZGlyRXhpc3RzQXN5bmMob3B0aW9ucy50eXBlRGVmRGlyKSkpIHtcbiAgICByZXR1cm4geyBleHBvcnRzOiBbXSwgZHRzOiAnJyB9XG4gIH1cblxuICBsZXQgaGVhZGVyID0gJydcbiAgbGV0IGR0cyA9ICcnXG4gIGxldCBleHBvcnRzOiBzdHJpbmdbXSA9IFtdXG5cbiAgaWYgKCFvcHRpb25zLm5vRHRzSGVhZGVyKSB7XG4gICAgY29uc3QgZHRzSGVhZGVyID0gb3B0aW9ucy5kdHNIZWFkZXIgPz8gb3B0aW9ucy5jb25maWdEdHNIZWFkZXJcbiAgICAvLyBgZHRzSGVhZGVyRmlsZWAgaW4gY29uZmlnID4gYGR0c0hlYWRlcmAgaW4gY2xpIGZsYWcgPiBgZHRzSGVhZGVyYCBpbiBjb25maWdcbiAgICBpZiAob3B0aW9ucy5jb25maWdEdHNIZWFkZXJGaWxlKSB7XG4gICAgICB0cnkge1xuICAgICAgICBoZWFkZXIgPSBhd2FpdCByZWFkRmlsZUFzeW5jKFxuICAgICAgICAgIGpvaW4ob3B0aW9ucy5jd2QsIG9wdGlvbnMuY29uZmlnRHRzSGVhZGVyRmlsZSksXG4gICAgICAgICAgJ3V0Zi04JyxcbiAgICAgICAgKVxuICAgICAgfSBjYXRjaCAoZSkge1xuICAgICAgICBkZWJ1Zy53YXJuKFxuICAgICAgICAgIGBGYWlsZWQgdG8gcmVhZCBkdHMgaGVhZGVyIGZpbGUgJHtvcHRpb25zLmNvbmZpZ0R0c0hlYWRlckZpbGV9YCxcbiAgICAgICAgICBlLFxuICAgICAgICApXG4gICAgICB9XG4gICAgfSBlbHNlIGlmIChkdHNIZWFkZXIpIHtcbiAgICAgIGhlYWRlciA9IGR0c0hlYWRlclxuICAgIH0gZWxzZSB7XG4gICAgICBoZWFkZXIgPSBERUZBVUxUX1RZUEVfREVGX0hFQURFUlxuICAgIH1cbiAgfVxuXG4gIGNvbnN0IGZpbGVzID0gYXdhaXQgcmVhZGRpckFzeW5jKG9wdGlvbnMudHlwZURlZkRpciwgeyB3aXRoRmlsZVR5cGVzOiB0cnVlIH0pXG5cbiAgaWYgKCFmaWxlcy5sZW5ndGgpIHtcbiAgICBkZWJ1ZygnTm8gdHlwZSBkZWYgZmlsZXMgZm91bmQuIFNraXAgZ2VuZXJhdGluZyBkdHMgZmlsZS4nKVxuICAgIHJldHVybiB7IGV4cG9ydHM6IFtdLCBkdHM6ICcnIH1cbiAgfVxuXG4gIGZvciAoY29uc3QgZmlsZSBvZiBmaWxlcykge1xuICAgIGlmICghZmlsZS5pc0ZpbGUoKSkge1xuICAgICAgY29udGludWVcbiAgICB9XG5cbiAgICBjb25zdCB7IGR0czogZmlsZUR0cywgZXhwb3J0czogZmlsZUV4cG9ydHMgfSA9IGF3YWl0IHByb2Nlc3NUeXBlRGVmKFxuICAgICAgam9pbihvcHRpb25zLnR5cGVEZWZEaXIsIGZpbGUubmFtZSksXG4gICAgICBvcHRpb25zLmNvbnN0RW51bSA/PyB0cnVlLFxuICAgIClcblxuICAgIGR0cyArPSBmaWxlRHRzXG4gICAgZXhwb3J0cy5wdXNoKC4uLmZpbGVFeHBvcnRzKVxuICB9XG5cbiAgaWYgKGR0cy5pbmRleE9mKCdFeHRlcm5hbE9iamVjdDwnKSA+IC0xKSB7XG4gICAgaGVhZGVyICs9IGBcbmV4cG9ydCBkZWNsYXJlIGNsYXNzIEV4dGVybmFsT2JqZWN0PFQ+IHtcbiAgcmVhZG9ubHkgJyc6IHtcbiAgICByZWFkb25seSAnJzogdW5pcXVlIHN5bWJvbFxuICAgIFtLOiBzeW1ib2xdOiBUXG4gIH1cbn1cbmBcbiAgfVxuXG4gIGlmIChkdHMuaW5kZXhPZignVHlwZWRBcnJheScpID4gLTEpIHtcbiAgICBoZWFkZXIgKz0gYFxuZXhwb3J0IHR5cGUgVHlwZWRBcnJheSA9IEludDhBcnJheSB8IFVpbnQ4QXJyYXkgfCBVaW50OENsYW1wZWRBcnJheSB8IEludDE2QXJyYXkgfCBVaW50MTZBcnJheSB8IEludDMyQXJyYXkgfCBVaW50MzJBcnJheSB8IEZsb2F0MzJBcnJheSB8IEZsb2F0NjRBcnJheSB8IEJpZ0ludDY0QXJyYXkgfCBCaWdVaW50NjRBcnJheVxuYFxuICB9XG5cbiAgZHRzID0gaGVhZGVyICsgZHRzXG5cbiAgcmV0dXJuIHtcbiAgICBleHBvcnRzLFxuICAgIGR0cyxcbiAgfVxufVxuIiwiLy8gVGhpcyBmaWxlIGlzIGdlbmVyYXRlZCBieSBjb2RlZ2VuL2luZGV4LnRzXG4vLyBEbyBub3QgZWRpdCB0aGlzIGZpbGUgbWFudWFsbHlcbmltcG9ydCB7IENvbW1hbmQsIE9wdGlvbiB9IGZyb20gJ2NsaXBhbmlvbidcblxuZXhwb3J0IGFic3RyYWN0IGNsYXNzIEJhc2VDcmVhdGVOcG1EaXJzQ29tbWFuZCBleHRlbmRzIENvbW1hbmQge1xuICBzdGF0aWMgcGF0aHMgPSBbWydjcmVhdGUtbnBtLWRpcnMnXV1cblxuICBzdGF0aWMgdXNhZ2UgPSBDb21tYW5kLlVzYWdlKHtcbiAgICBkZXNjcmlwdGlvbjogJ0NyZWF0ZSBucG0gcGFja2FnZSBkaXJzIGZvciBkaWZmZXJlbnQgcGxhdGZvcm1zJyxcbiAgfSlcblxuICBjd2QgPSBPcHRpb24uU3RyaW5nKCctLWN3ZCcsIHByb2Nlc3MuY3dkKCksIHtcbiAgICBkZXNjcmlwdGlvbjpcbiAgICAgICdUaGUgd29ya2luZyBkaXJlY3Rvcnkgb2Ygd2hlcmUgbmFwaSBjb21tYW5kIHdpbGwgYmUgZXhlY3V0ZWQgaW4sIGFsbCBvdGhlciBwYXRocyBvcHRpb25zIGFyZSByZWxhdGl2ZSB0byB0aGlzIHBhdGgnLFxuICB9KVxuXG4gIGNvbmZpZ1BhdGg/OiBzdHJpbmcgPSBPcHRpb24uU3RyaW5nKCctLWNvbmZpZy1wYXRoLC1jJywge1xuICAgIGRlc2NyaXB0aW9uOiAnUGF0aCB0byBgbmFwaWAgY29uZmlnIGpzb24gZmlsZScsXG4gIH0pXG5cbiAgcGFja2FnZUpzb25QYXRoID0gT3B0aW9uLlN0cmluZygnLS1wYWNrYWdlLWpzb24tcGF0aCcsICdwYWNrYWdlLmpzb24nLCB7XG4gICAgZGVzY3JpcHRpb246ICdQYXRoIHRvIGBwYWNrYWdlLmpzb25gJyxcbiAgfSlcblxuICBucG1EaXIgPSBPcHRpb24uU3RyaW5nKCctLW5wbS1kaXInLCAnbnBtJywge1xuICAgIGRlc2NyaXB0aW9uOiAnUGF0aCB0byB0aGUgZm9sZGVyIHdoZXJlIHRoZSBucG0gcGFja2FnZXMgcHV0JyxcbiAgfSlcblxuICBkcnlSdW4gPSBPcHRpb24uQm9vbGVhbignLS1kcnktcnVuJywgZmFsc2UsIHtcbiAgICBkZXNjcmlwdGlvbjogJ0RyeSBydW4gd2l0aG91dCB0b3VjaGluZyBmaWxlIHN5c3RlbScsXG4gIH0pXG5cbiAgZ2V0T3B0aW9ucygpIHtcbiAgICByZXR1cm4ge1xuICAgICAgY3dkOiB0aGlzLmN3ZCxcbiAgICAgIGNvbmZpZ1BhdGg6IHRoaXMuY29uZmlnUGF0aCxcbiAgICAgIHBhY2thZ2VKc29uUGF0aDogdGhpcy5wYWNrYWdlSnNvblBhdGgsXG4gICAgICBucG1EaXI6IHRoaXMubnBtRGlyLFxuICAgICAgZHJ5UnVuOiB0aGlzLmRyeVJ1bixcbiAgICB9XG4gIH1cbn1cblxuLyoqXG4gKiBDcmVhdGUgbnBtIHBhY2thZ2UgZGlycyBmb3IgZGlmZmVyZW50IHBsYXRmb3Jtc1xuICovXG5leHBvcnQgaW50ZXJmYWNlIENyZWF0ZU5wbURpcnNPcHRpb25zIHtcbiAgLyoqXG4gICAqIFRoZSB3b3JraW5nIGRpcmVjdG9yeSBvZiB3aGVyZSBuYXBpIGNvbW1hbmQgd2lsbCBiZSBleGVjdXRlZCBpbiwgYWxsIG90aGVyIHBhdGhzIG9wdGlvbnMgYXJlIHJlbGF0aXZlIHRvIHRoaXMgcGF0aFxuICAgKlxuICAgKiBAZGVmYXVsdCBwcm9jZXNzLmN3ZCgpXG4gICAqL1xuICBjd2Q/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gYG5hcGlgIGNvbmZpZyBqc29uIGZpbGVcbiAgICovXG4gIGNvbmZpZ1BhdGg/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gYHBhY2thZ2UuanNvbmBcbiAgICpcbiAgICogQGRlZmF1bHQgJ3BhY2thZ2UuanNvbidcbiAgICovXG4gIHBhY2thZ2VKc29uUGF0aD86IHN0cmluZ1xuICAvKipcbiAgICogUGF0aCB0byB0aGUgZm9sZGVyIHdoZXJlIHRoZSBucG0gcGFja2FnZXMgcHV0XG4gICAqXG4gICAqIEBkZWZhdWx0ICducG0nXG4gICAqL1xuICBucG1EaXI/OiBzdHJpbmdcbiAgLyoqXG4gICAqIERyeSBydW4gd2l0aG91dCB0b3VjaGluZyBmaWxlIHN5c3RlbVxuICAgKlxuICAgKiBAZGVmYXVsdCBmYWxzZVxuICAgKi9cbiAgZHJ5UnVuPzogYm9vbGVhblxufVxuXG5leHBvcnQgZnVuY3Rpb24gYXBwbHlEZWZhdWx0Q3JlYXRlTnBtRGlyc09wdGlvbnMoXG4gIG9wdGlvbnM6IENyZWF0ZU5wbURpcnNPcHRpb25zLFxuKSB7XG4gIHJldHVybiB7XG4gICAgY3dkOiBwcm9jZXNzLmN3ZCgpLFxuICAgIHBhY2thZ2VKc29uUGF0aDogJ3BhY2thZ2UuanNvbicsXG4gICAgbnBtRGlyOiAnbnBtJyxcbiAgICBkcnlSdW46IGZhbHNlLFxuICAgIC4uLm9wdGlvbnMsXG4gIH1cbn1cbiIsImltcG9ydCB7IGpvaW4sIHJlc29sdmUgfSBmcm9tICdub2RlOnBhdGgnXG5cbmltcG9ydCB7IHBhcnNlIH0gZnJvbSAnc2VtdmVyJ1xuXG5pbXBvcnQge1xuICBhcHBseURlZmF1bHRDcmVhdGVOcG1EaXJzT3B0aW9ucyxcbiAgdHlwZSBDcmVhdGVOcG1EaXJzT3B0aW9ucyxcbn0gZnJvbSAnLi4vZGVmL2NyZWF0ZS1ucG0tZGlycy5qcydcbmltcG9ydCB7XG4gIGRlYnVnRmFjdG9yeSxcbiAgcmVhZE5hcGlDb25maWcsXG4gIG1rZGlyQXN5bmMgYXMgcmF3TWtkaXJBc3luYyxcbiAgcGljayxcbiAgd3JpdGVGaWxlQXN5bmMgYXMgcmF3V3JpdGVGaWxlQXN5bmMsXG4gIHR5cGUgVGFyZ2V0LFxuICB0eXBlIENvbW1vblBhY2thZ2VKc29uRmllbGRzLFxufSBmcm9tICcuLi91dGlscy9pbmRleC5qcydcblxuY29uc3QgZGVidWcgPSBkZWJ1Z0ZhY3RvcnkoJ2NyZWF0ZS1ucG0tZGlycycpXG5cbmV4cG9ydCBpbnRlcmZhY2UgUGFja2FnZU1ldGEge1xuICAnZGlzdC10YWdzJzogeyBbaW5kZXg6IHN0cmluZ106IHN0cmluZyB9XG59XG5cbmV4cG9ydCBhc3luYyBmdW5jdGlvbiBjcmVhdGVOcG1EaXJzKHVzZXJPcHRpb25zOiBDcmVhdGVOcG1EaXJzT3B0aW9ucykge1xuICBjb25zdCBvcHRpb25zID0gYXBwbHlEZWZhdWx0Q3JlYXRlTnBtRGlyc09wdGlvbnModXNlck9wdGlvbnMpXG5cbiAgYXN5bmMgZnVuY3Rpb24gbWtkaXJBc3luYyhkaXI6IHN0cmluZykge1xuICAgIGRlYnVnKCdUcnkgdG8gY3JlYXRlIGRpcjogJWknLCBkaXIpXG4gICAgaWYgKG9wdGlvbnMuZHJ5UnVuKSB7XG4gICAgICByZXR1cm5cbiAgICB9XG5cbiAgICBhd2FpdCByYXdNa2RpckFzeW5jKGRpciwge1xuICAgICAgcmVjdXJzaXZlOiB0cnVlLFxuICAgIH0pXG4gIH1cblxuICBhc3luYyBmdW5jdGlvbiB3cml0ZUZpbGVBc3luYyhmaWxlOiBzdHJpbmcsIGNvbnRlbnQ6IHN0cmluZykge1xuICAgIGRlYnVnKCdXcml0aW5nIGZpbGUgJWknLCBmaWxlKVxuXG4gICAgaWYgKG9wdGlvbnMuZHJ5UnVuKSB7XG4gICAgICBkZWJ1Zyhjb250ZW50KVxuICAgICAgcmV0dXJuXG4gICAgfVxuXG4gICAgYXdhaXQgcmF3V3JpdGVGaWxlQXN5bmMoZmlsZSwgY29udGVudClcbiAgfVxuXG4gIGNvbnN0IHBhY2thZ2VKc29uUGF0aCA9IHJlc29sdmUob3B0aW9ucy5jd2QsIG9wdGlvbnMucGFja2FnZUpzb25QYXRoKVxuICBjb25zdCBucG1QYXRoID0gcmVzb2x2ZShvcHRpb25zLmN3ZCwgb3B0aW9ucy5ucG1EaXIpXG5cbiAgZGVidWcoYFJlYWQgY29udGVudCBmcm9tIFske29wdGlvbnMuY29uZmlnUGF0aCA/PyBwYWNrYWdlSnNvblBhdGh9XWApXG5cbiAgY29uc3QgeyB0YXJnZXRzLCBiaW5hcnlOYW1lLCBwYWNrYWdlTmFtZSwgcGFja2FnZUpzb24gfSA9XG4gICAgYXdhaXQgcmVhZE5hcGlDb25maWcoXG4gICAgICBwYWNrYWdlSnNvblBhdGgsXG4gICAgICBvcHRpb25zLmNvbmZpZ1BhdGggPyByZXNvbHZlKG9wdGlvbnMuY3dkLCBvcHRpb25zLmNvbmZpZ1BhdGgpIDogdW5kZWZpbmVkLFxuICAgIClcblxuICBmb3IgKGNvbnN0IHRhcmdldCBvZiB0YXJnZXRzKSB7XG4gICAgY29uc3QgdGFyZ2V0RGlyID0gam9pbihucG1QYXRoLCBgJHt0YXJnZXQucGxhdGZvcm1BcmNoQUJJfWApXG4gICAgYXdhaXQgbWtkaXJBc3luYyh0YXJnZXREaXIpXG5cbiAgICBjb25zdCBiaW5hcnlGaWxlTmFtZSA9XG4gICAgICB0YXJnZXQuYXJjaCA9PT0gJ3dhc20zMidcbiAgICAgICAgPyBgJHtiaW5hcnlOYW1lfS4ke3RhcmdldC5wbGF0Zm9ybUFyY2hBQkl9Lndhc21gXG4gICAgICAgIDogYCR7YmluYXJ5TmFtZX0uJHt0YXJnZXQucGxhdGZvcm1BcmNoQUJJfS5ub2RlYFxuICAgIGNvbnN0IHNjb3BlZFBhY2thZ2VKc29uOiBDb21tb25QYWNrYWdlSnNvbkZpZWxkcyA9IHtcbiAgICAgIG5hbWU6IGAke3BhY2thZ2VOYW1lfS0ke3RhcmdldC5wbGF0Zm9ybUFyY2hBQkl9YCxcbiAgICAgIHZlcnNpb246IHBhY2thZ2VKc29uLnZlcnNpb24sXG4gICAgICBjcHU6IHRhcmdldC5hcmNoICE9PSAndW5pdmVyc2FsJyA/IFt0YXJnZXQuYXJjaF0gOiB1bmRlZmluZWQsXG4gICAgICBtYWluOiBiaW5hcnlGaWxlTmFtZSxcbiAgICAgIGZpbGVzOiBbYmluYXJ5RmlsZU5hbWVdLFxuICAgICAgLi4ucGljayhcbiAgICAgICAgcGFja2FnZUpzb24sXG4gICAgICAgICdkZXNjcmlwdGlvbicsXG4gICAgICAgICdrZXl3b3JkcycsXG4gICAgICAgICdhdXRob3InLFxuICAgICAgICAnYXV0aG9ycycsXG4gICAgICAgICdob21lcGFnZScsXG4gICAgICAgICdsaWNlbnNlJyxcbiAgICAgICAgJ2VuZ2luZXMnLFxuICAgICAgICAncmVwb3NpdG9yeScsXG4gICAgICAgICdidWdzJyxcbiAgICAgICksXG4gICAgfVxuICAgIGlmIChwYWNrYWdlSnNvbi5wdWJsaXNoQ29uZmlnKSB7XG4gICAgICBzY29wZWRQYWNrYWdlSnNvbi5wdWJsaXNoQ29uZmlnID0gcGljayhcbiAgICAgICAgcGFja2FnZUpzb24ucHVibGlzaENvbmZpZyxcbiAgICAgICAgJ3JlZ2lzdHJ5JyxcbiAgICAgICAgJ2FjY2VzcycsXG4gICAgICApXG4gICAgfVxuICAgIGlmICh0YXJnZXQuYXJjaCAhPT0gJ3dhc20zMicpIHtcbiAgICAgIHNjb3BlZFBhY2thZ2VKc29uLm9zID0gW3RhcmdldC5wbGF0Zm9ybV1cbiAgICB9IGVsc2Uge1xuICAgICAgY29uc3QgZW50cnkgPSBgJHtiaW5hcnlOYW1lfS53YXNpLmNqc2BcbiAgICAgIHNjb3BlZFBhY2thZ2VKc29uLm1haW4gPSBlbnRyeVxuICAgICAgc2NvcGVkUGFja2FnZUpzb24uYnJvd3NlciA9IGAke2JpbmFyeU5hbWV9Lndhc2ktYnJvd3Nlci5qc2BcbiAgICAgIHNjb3BlZFBhY2thZ2VKc29uLmZpbGVzPy5wdXNoKFxuICAgICAgICBlbnRyeSxcbiAgICAgICAgc2NvcGVkUGFja2FnZUpzb24uYnJvd3NlcixcbiAgICAgICAgYHdhc2ktd29ya2VyLm1qc2AsXG4gICAgICAgIGB3YXNpLXdvcmtlci1icm93c2VyLm1qc2AsXG4gICAgICApXG4gICAgICBsZXQgbmVlZFJlc3RyaWN0Tm9kZVZlcnNpb24gPSB0cnVlXG4gICAgICBpZiAoc2NvcGVkUGFja2FnZUpzb24uZW5naW5lcz8ubm9kZSkge1xuICAgICAgICB0cnkge1xuICAgICAgICAgIGNvbnN0IHsgbWFqb3IgfSA9IHBhcnNlKHNjb3BlZFBhY2thZ2VKc29uLmVuZ2luZXMubm9kZSkgPz8ge1xuICAgICAgICAgICAgbWFqb3I6IDAsXG4gICAgICAgICAgfVxuICAgICAgICAgIGlmIChtYWpvciA+PSAxNCkge1xuICAgICAgICAgICAgbmVlZFJlc3RyaWN0Tm9kZVZlcnNpb24gPSBmYWxzZVxuICAgICAgICAgIH1cbiAgICAgICAgfSBjYXRjaCB7XG4gICAgICAgICAgLy8gaWdub3JlXG4gICAgICAgIH1cbiAgICAgIH1cbiAgICAgIGlmIChuZWVkUmVzdHJpY3ROb2RlVmVyc2lvbikge1xuICAgICAgICBzY29wZWRQYWNrYWdlSnNvbi5lbmdpbmVzID0ge1xuICAgICAgICAgIG5vZGU6ICc+PTE0LjAuMCcsXG4gICAgICAgIH1cbiAgICAgIH1cbiAgICAgIGNvbnN0IHdhc21SdW50aW1lID0gYXdhaXQgZmV0Y2goXG4gICAgICAgIGBodHRwczovL3JlZ2lzdHJ5Lm5wbWpzLm9yZy9AbmFwaS1ycy93YXNtLXJ1bnRpbWVgLFxuICAgICAgKS50aGVuKChyZXMpID0+IHJlcy5qc29uKCkgYXMgUHJvbWlzZTxQYWNrYWdlTWV0YT4pXG4gICAgICBzY29wZWRQYWNrYWdlSnNvbi5kZXBlbmRlbmNpZXMgPSB7XG4gICAgICAgICdAbmFwaS1ycy93YXNtLXJ1bnRpbWUnOiBgXiR7d2FzbVJ1bnRpbWVbJ2Rpc3QtdGFncyddLmxhdGVzdH1gLFxuICAgICAgfVxuICAgIH1cblxuICAgIGlmICh0YXJnZXQuYWJpID09PSAnZ251Jykge1xuICAgICAgc2NvcGVkUGFja2FnZUpzb24ubGliYyA9IFsnZ2xpYmMnXVxuICAgIH0gZWxzZSBpZiAodGFyZ2V0LmFiaSA9PT0gJ211c2wnKSB7XG4gICAgICBzY29wZWRQYWNrYWdlSnNvbi5saWJjID0gWydtdXNsJ11cbiAgICB9XG5cbiAgICBjb25zdCB0YXJnZXRQYWNrYWdlSnNvbiA9IGpvaW4odGFyZ2V0RGlyLCAncGFja2FnZS5qc29uJylcbiAgICBhd2FpdCB3cml0ZUZpbGVBc3luYyhcbiAgICAgIHRhcmdldFBhY2thZ2VKc29uLFxuICAgICAgSlNPTi5zdHJpbmdpZnkoc2NvcGVkUGFja2FnZUpzb24sIG51bGwsIDIpICsgJ1xcbicsXG4gICAgKVxuICAgIGNvbnN0IHRhcmdldFJlYWRtZSA9IGpvaW4odGFyZ2V0RGlyLCAnUkVBRE1FLm1kJylcbiAgICBhd2FpdCB3cml0ZUZpbGVBc3luYyh0YXJnZXRSZWFkbWUsIHJlYWRtZShwYWNrYWdlTmFtZSwgdGFyZ2V0KSlcblxuICAgIGRlYnVnLmluZm8oYCR7cGFja2FnZU5hbWV9IC0ke3RhcmdldC5wbGF0Zm9ybUFyY2hBQkl9IGNyZWF0ZWRgKVxuICB9XG59XG5cbmZ1bmN0aW9uIHJlYWRtZShwYWNrYWdlTmFtZTogc3RyaW5nLCB0YXJnZXQ6IFRhcmdldCkge1xuICByZXR1cm4gYCMgXFxgJHtwYWNrYWdlTmFtZX0tJHt0YXJnZXQucGxhdGZvcm1BcmNoQUJJfVxcYFxuXG5UaGlzIGlzIHRoZSAqKiR7dGFyZ2V0LnRyaXBsZX0qKiBiaW5hcnkgZm9yIFxcYCR7cGFja2FnZU5hbWV9XFxgXG5gXG59XG4iLCIvLyBUaGlzIGZpbGUgaXMgZ2VuZXJhdGVkIGJ5IGNvZGVnZW4vaW5kZXgudHNcbi8vIERvIG5vdCBlZGl0IHRoaXMgZmlsZSBtYW51YWxseVxuaW1wb3J0IHsgQ29tbWFuZCwgT3B0aW9uIH0gZnJvbSAnY2xpcGFuaW9uJ1xuaW1wb3J0ICogYXMgdHlwYW5pb24gZnJvbSAndHlwYW5pb24nXG5cbmV4cG9ydCBhYnN0cmFjdCBjbGFzcyBCYXNlTmV3Q29tbWFuZCBleHRlbmRzIENvbW1hbmQge1xuICBzdGF0aWMgcGF0aHMgPSBbWyduZXcnXV1cblxuICBzdGF0aWMgdXNhZ2UgPSBDb21tYW5kLlVzYWdlKHtcbiAgICBkZXNjcmlwdGlvbjogJ0NyZWF0ZSBhIG5ldyBwcm9qZWN0IHdpdGggcHJlLWNvbmZpZ3VyZWQgYm9pbGVycGxhdGUnLFxuICB9KVxuXG4gICQkcGF0aCA9IE9wdGlvbi5TdHJpbmcoeyByZXF1aXJlZDogZmFsc2UgfSlcblxuICAkJG5hbWU/OiBzdHJpbmcgPSBPcHRpb24uU3RyaW5nKCctLW5hbWUsLW4nLCB7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnVGhlIG5hbWUgb2YgdGhlIHByb2plY3QsIGRlZmF1bHQgdG8gdGhlIG5hbWUgb2YgdGhlIGRpcmVjdG9yeSBpZiBub3QgcHJvdmlkZWQnLFxuICB9KVxuXG4gIG1pbk5vZGVBcGlWZXJzaW9uID0gT3B0aW9uLlN0cmluZygnLS1taW4tbm9kZS1hcGksLXYnLCAnNCcsIHtcbiAgICB2YWxpZGF0b3I6IHR5cGFuaW9uLmlzTnVtYmVyKCksXG4gICAgZGVzY3JpcHRpb246ICdUaGUgbWluaW11bSBOb2RlLUFQSSB2ZXJzaW9uIHRvIHN1cHBvcnQnLFxuICB9KVxuXG4gIHBhY2thZ2VNYW5hZ2VyID0gT3B0aW9uLlN0cmluZygnLS1wYWNrYWdlLW1hbmFnZXInLCAneWFybicsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1RoZSBwYWNrYWdlIG1hbmFnZXIgdG8gdXNlLiBPbmx5IHN1cHBvcnQgeWFybiA0LnggZm9yIG5vdy4nLFxuICB9KVxuXG4gIGxpY2Vuc2UgPSBPcHRpb24uU3RyaW5nKCctLWxpY2Vuc2UsLWwnLCAnTUlUJywge1xuICAgIGRlc2NyaXB0aW9uOiAnTGljZW5zZSBmb3Igb3Blbi1zb3VyY2VkIHByb2plY3QnLFxuICB9KVxuXG4gIHRhcmdldHMgPSBPcHRpb24uQXJyYXkoJy0tdGFyZ2V0cywtdCcsIFtdLCB7XG4gICAgZGVzY3JpcHRpb246ICdBbGwgdGFyZ2V0cyB0aGUgY3JhdGUgd2lsbCBiZSBjb21waWxlZCBmb3IuJyxcbiAgfSlcblxuICBlbmFibGVEZWZhdWx0VGFyZ2V0cyA9IE9wdGlvbi5Cb29sZWFuKCctLWVuYWJsZS1kZWZhdWx0LXRhcmdldHMnLCB0cnVlLCB7XG4gICAgZGVzY3JpcHRpb246ICdXaGV0aGVyIGVuYWJsZSBkZWZhdWx0IHRhcmdldHMnLFxuICB9KVxuXG4gIGVuYWJsZUFsbFRhcmdldHMgPSBPcHRpb24uQm9vbGVhbignLS1lbmFibGUtYWxsLXRhcmdldHMnLCBmYWxzZSwge1xuICAgIGRlc2NyaXB0aW9uOiAnV2hldGhlciBlbmFibGUgYWxsIHRhcmdldHMnLFxuICB9KVxuXG4gIGVuYWJsZVR5cGVEZWYgPSBPcHRpb24uQm9vbGVhbignLS1lbmFibGUtdHlwZS1kZWYnLCB0cnVlLCB7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnV2hldGhlciBlbmFibGUgdGhlIGB0eXBlLWRlZmAgZmVhdHVyZSBmb3IgdHlwZXNjcmlwdCBkZWZpbml0aW9ucyBhdXRvLWdlbmVyYXRpb24nLFxuICB9KVxuXG4gIGVuYWJsZUdpdGh1YkFjdGlvbnMgPSBPcHRpb24uQm9vbGVhbignLS1lbmFibGUtZ2l0aHViLWFjdGlvbnMnLCB0cnVlLCB7XG4gICAgZGVzY3JpcHRpb246ICdXaGV0aGVyIGdlbmVyYXRlIHByZWNvbmZpZ3VyZWQgR2l0SHViIEFjdGlvbnMgd29ya2Zsb3cnLFxuICB9KVxuXG4gIHRlc3RGcmFtZXdvcmsgPSBPcHRpb24uU3RyaW5nKCctLXRlc3QtZnJhbWV3b3JrJywgJ2F2YScsIHtcbiAgICBkZXNjcmlwdGlvbjpcbiAgICAgICdUaGUgSmF2YVNjcmlwdCB0ZXN0IGZyYW1ld29yayB0byB1c2UsIG9ubHkgc3VwcG9ydCBgYXZhYCBmb3Igbm93JyxcbiAgfSlcblxuICBkcnlSdW4gPSBPcHRpb24uQm9vbGVhbignLS1kcnktcnVuJywgZmFsc2UsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1doZXRoZXIgdG8gcnVuIHRoZSBjb21tYW5kIGluIGRyeS1ydW4gbW9kZScsXG4gIH0pXG5cbiAgZ2V0T3B0aW9ucygpIHtcbiAgICByZXR1cm4ge1xuICAgICAgcGF0aDogdGhpcy4kJHBhdGgsXG4gICAgICBuYW1lOiB0aGlzLiQkbmFtZSxcbiAgICAgIG1pbk5vZGVBcGlWZXJzaW9uOiB0aGlzLm1pbk5vZGVBcGlWZXJzaW9uLFxuICAgICAgcGFja2FnZU1hbmFnZXI6IHRoaXMucGFja2FnZU1hbmFnZXIsXG4gICAgICBsaWNlbnNlOiB0aGlzLmxpY2Vuc2UsXG4gICAgICB0YXJnZXRzOiB0aGlzLnRhcmdldHMsXG4gICAgICBlbmFibGVEZWZhdWx0VGFyZ2V0czogdGhpcy5lbmFibGVEZWZhdWx0VGFyZ2V0cyxcbiAgICAgIGVuYWJsZUFsbFRhcmdldHM6IHRoaXMuZW5hYmxlQWxsVGFyZ2V0cyxcbiAgICAgIGVuYWJsZVR5cGVEZWY6IHRoaXMuZW5hYmxlVHlwZURlZixcbiAgICAgIGVuYWJsZUdpdGh1YkFjdGlvbnM6IHRoaXMuZW5hYmxlR2l0aHViQWN0aW9ucyxcbiAgICAgIHRlc3RGcmFtZXdvcms6IHRoaXMudGVzdEZyYW1ld29yayxcbiAgICAgIGRyeVJ1bjogdGhpcy5kcnlSdW4sXG4gICAgfVxuICB9XG59XG5cbi8qKlxuICogQ3JlYXRlIGEgbmV3IHByb2plY3Qgd2l0aCBwcmUtY29uZmlndXJlZCBib2lsZXJwbGF0ZVxuICovXG5leHBvcnQgaW50ZXJmYWNlIE5ld09wdGlvbnMge1xuICAvKipcbiAgICogVGhlIHBhdGggd2hlcmUgdGhlIE5BUEktUlMgcHJvamVjdCB3aWxsIGJlIGNyZWF0ZWQuXG4gICAqL1xuICBwYXRoPzogc3RyaW5nXG4gIC8qKlxuICAgKiBUaGUgbmFtZSBvZiB0aGUgcHJvamVjdCwgZGVmYXVsdCB0byB0aGUgbmFtZSBvZiB0aGUgZGlyZWN0b3J5IGlmIG5vdCBwcm92aWRlZFxuICAgKi9cbiAgbmFtZT86IHN0cmluZ1xuICAvKipcbiAgICogVGhlIG1pbmltdW0gTm9kZS1BUEkgdmVyc2lvbiB0byBzdXBwb3J0XG4gICAqXG4gICAqIEBkZWZhdWx0IDRcbiAgICovXG4gIG1pbk5vZGVBcGlWZXJzaW9uPzogbnVtYmVyXG4gIC8qKlxuICAgKiBUaGUgcGFja2FnZSBtYW5hZ2VyIHRvIHVzZS4gT25seSBzdXBwb3J0IHlhcm4gNC54IGZvciBub3cuXG4gICAqXG4gICAqIEBkZWZhdWx0ICd5YXJuJ1xuICAgKi9cbiAgcGFja2FnZU1hbmFnZXI/OiBzdHJpbmdcbiAgLyoqXG4gICAqIExpY2Vuc2UgZm9yIG9wZW4tc291cmNlZCBwcm9qZWN0XG4gICAqXG4gICAqIEBkZWZhdWx0ICdNSVQnXG4gICAqL1xuICBsaWNlbnNlPzogc3RyaW5nXG4gIC8qKlxuICAgKiBBbGwgdGFyZ2V0cyB0aGUgY3JhdGUgd2lsbCBiZSBjb21waWxlZCBmb3IuXG4gICAqXG4gICAqIEBkZWZhdWx0IFtdXG4gICAqL1xuICB0YXJnZXRzPzogc3RyaW5nW11cbiAgLyoqXG4gICAqIFdoZXRoZXIgZW5hYmxlIGRlZmF1bHQgdGFyZ2V0c1xuICAgKlxuICAgKiBAZGVmYXVsdCB0cnVlXG4gICAqL1xuICBlbmFibGVEZWZhdWx0VGFyZ2V0cz86IGJvb2xlYW5cbiAgLyoqXG4gICAqIFdoZXRoZXIgZW5hYmxlIGFsbCB0YXJnZXRzXG4gICAqXG4gICAqIEBkZWZhdWx0IGZhbHNlXG4gICAqL1xuICBlbmFibGVBbGxUYXJnZXRzPzogYm9vbGVhblxuICAvKipcbiAgICogV2hldGhlciBlbmFibGUgdGhlIGB0eXBlLWRlZmAgZmVhdHVyZSBmb3IgdHlwZXNjcmlwdCBkZWZpbml0aW9ucyBhdXRvLWdlbmVyYXRpb25cbiAgICpcbiAgICogQGRlZmF1bHQgdHJ1ZVxuICAgKi9cbiAgZW5hYmxlVHlwZURlZj86IGJvb2xlYW5cbiAgLyoqXG4gICAqIFdoZXRoZXIgZ2VuZXJhdGUgcHJlY29uZmlndXJlZCBHaXRIdWIgQWN0aW9ucyB3b3JrZmxvd1xuICAgKlxuICAgKiBAZGVmYXVsdCB0cnVlXG4gICAqL1xuICBlbmFibGVHaXRodWJBY3Rpb25zPzogYm9vbGVhblxuICAvKipcbiAgICogVGhlIEphdmFTY3JpcHQgdGVzdCBmcmFtZXdvcmsgdG8gdXNlLCBvbmx5IHN1cHBvcnQgYGF2YWAgZm9yIG5vd1xuICAgKlxuICAgKiBAZGVmYXVsdCAnYXZhJ1xuICAgKi9cbiAgdGVzdEZyYW1ld29yaz86IHN0cmluZ1xuICAvKipcbiAgICogV2hldGhlciB0byBydW4gdGhlIGNvbW1hbmQgaW4gZHJ5LXJ1biBtb2RlXG4gICAqXG4gICAqIEBkZWZhdWx0IGZhbHNlXG4gICAqL1xuICBkcnlSdW4/OiBib29sZWFuXG59XG5cbmV4cG9ydCBmdW5jdGlvbiBhcHBseURlZmF1bHROZXdPcHRpb25zKG9wdGlvbnM6IE5ld09wdGlvbnMpIHtcbiAgcmV0dXJuIHtcbiAgICBtaW5Ob2RlQXBpVmVyc2lvbjogNCxcbiAgICBwYWNrYWdlTWFuYWdlcjogJ3lhcm4nLFxuICAgIGxpY2Vuc2U6ICdNSVQnLFxuICAgIHRhcmdldHM6IFtdLFxuICAgIGVuYWJsZURlZmF1bHRUYXJnZXRzOiB0cnVlLFxuICAgIGVuYWJsZUFsbFRhcmdldHM6IGZhbHNlLFxuICAgIGVuYWJsZVR5cGVEZWY6IHRydWUsXG4gICAgZW5hYmxlR2l0aHViQWN0aW9uczogdHJ1ZSxcbiAgICB0ZXN0RnJhbWV3b3JrOiAnYXZhJyxcbiAgICBkcnlSdW46IGZhbHNlLFxuICAgIC4uLm9wdGlvbnMsXG4gIH1cbn1cbiIsIi8vIENvcHlyaWdodCAyMDE4LTIwMjUgdGhlIERlbm8gYXV0aG9ycy4gTUlUIGxpY2Vuc2UuXG4vLyBUaGlzIG1vZHVsZSBpcyBicm93c2VyIGNvbXBhdGlibGUuXG4vLyBCYXJlIGtleXMgbWF5IG9ubHkgY29udGFpbiBBU0NJSSBsZXR0ZXJzLFxuLy8gQVNDSUkgZGlnaXRzLCB1bmRlcnNjb3JlcywgYW5kIGRhc2hlcyAoQS1aYS16MC05Xy0pLlxuZnVuY3Rpb24gam9pbktleXMoa2V5cykge1xuICAvLyBEb3R0ZWQga2V5cyBhcmUgYSBzZXF1ZW5jZSBvZiBiYXJlIG9yIHF1b3RlZCBrZXlzIGpvaW5lZCB3aXRoIGEgZG90LlxuICAvLyBUaGlzIGFsbG93cyBmb3IgZ3JvdXBpbmcgc2ltaWxhciBwcm9wZXJ0aWVzIHRvZ2V0aGVyOlxuICByZXR1cm4ga2V5cy5tYXAoKHN0cik9PntcbiAgICByZXR1cm4gc3RyLmxlbmd0aCA9PT0gMCB8fCBzdHIubWF0Y2goL1teQS1aYS16MC05Xy1dLykgPyBKU09OLnN0cmluZ2lmeShzdHIpIDogc3RyO1xuICB9KS5qb2luKFwiLlwiKTtcbn1cbmNsYXNzIER1bXBlciB7XG4gIG1heFBhZCA9IDA7XG4gIHNyY09iamVjdDtcbiAgb3V0cHV0ID0gW107XG4gICNhcnJheVR5cGVDYWNoZSA9IG5ldyBNYXAoKTtcbiAgY29uc3RydWN0b3Ioc3JjT2JqYyl7XG4gICAgdGhpcy5zcmNPYmplY3QgPSBzcmNPYmpjO1xuICB9XG4gIGR1bXAoZm10T3B0aW9ucyA9IHt9KSB7XG4gICAgLy8gZGVuby1saW50LWlnbm9yZSBuby1leHBsaWNpdC1hbnlcbiAgICB0aGlzLm91dHB1dCA9IHRoaXMuI3ByaW50T2JqZWN0KHRoaXMuc3JjT2JqZWN0KTtcbiAgICB0aGlzLm91dHB1dCA9IHRoaXMuI2Zvcm1hdChmbXRPcHRpb25zKTtcbiAgICByZXR1cm4gdGhpcy5vdXRwdXQ7XG4gIH1cbiAgI3ByaW50T2JqZWN0KG9iaiwga2V5cyA9IFtdKSB7XG4gICAgY29uc3Qgb3V0ID0gW107XG4gICAgY29uc3QgcHJvcHMgPSBPYmplY3Qua2V5cyhvYmopO1xuICAgIGNvbnN0IGlubGluZVByb3BzID0gW107XG4gICAgY29uc3QgbXVsdGlsaW5lUHJvcHMgPSBbXTtcbiAgICBmb3IgKGNvbnN0IHByb3Agb2YgcHJvcHMpe1xuICAgICAgaWYgKHRoaXMuI2lzU2ltcGx5U2VyaWFsaXphYmxlKG9ialtwcm9wXSkpIHtcbiAgICAgICAgaW5saW5lUHJvcHMucHVzaChwcm9wKTtcbiAgICAgIH0gZWxzZSB7XG4gICAgICAgIG11bHRpbGluZVByb3BzLnB1c2gocHJvcCk7XG4gICAgICB9XG4gICAgfVxuICAgIGNvbnN0IHNvcnRlZFByb3BzID0gaW5saW5lUHJvcHMuY29uY2F0KG11bHRpbGluZVByb3BzKTtcbiAgICBmb3IgKGNvbnN0IHByb3Agb2Ygc29ydGVkUHJvcHMpe1xuICAgICAgY29uc3QgdmFsdWUgPSBvYmpbcHJvcF07XG4gICAgICBpZiAodmFsdWUgaW5zdGFuY2VvZiBEYXRlKSB7XG4gICAgICAgIG91dC5wdXNoKHRoaXMuI2RhdGVEZWNsYXJhdGlvbihbXG4gICAgICAgICAgcHJvcFxuICAgICAgICBdLCB2YWx1ZSkpO1xuICAgICAgfSBlbHNlIGlmICh0eXBlb2YgdmFsdWUgPT09IFwic3RyaW5nXCIgfHwgdmFsdWUgaW5zdGFuY2VvZiBSZWdFeHApIHtcbiAgICAgICAgb3V0LnB1c2godGhpcy4jc3RyRGVjbGFyYXRpb24oW1xuICAgICAgICAgIHByb3BcbiAgICAgICAgXSwgdmFsdWUudG9TdHJpbmcoKSkpO1xuICAgICAgfSBlbHNlIGlmICh0eXBlb2YgdmFsdWUgPT09IFwibnVtYmVyXCIpIHtcbiAgICAgICAgb3V0LnB1c2godGhpcy4jbnVtYmVyRGVjbGFyYXRpb24oW1xuICAgICAgICAgIHByb3BcbiAgICAgICAgXSwgdmFsdWUpKTtcbiAgICAgIH0gZWxzZSBpZiAodHlwZW9mIHZhbHVlID09PSBcImJvb2xlYW5cIikge1xuICAgICAgICBvdXQucHVzaCh0aGlzLiNib29sRGVjbGFyYXRpb24oW1xuICAgICAgICAgIHByb3BcbiAgICAgICAgXSwgdmFsdWUpKTtcbiAgICAgIH0gZWxzZSBpZiAodmFsdWUgaW5zdGFuY2VvZiBBcnJheSkge1xuICAgICAgICBjb25zdCBhcnJheVR5cGUgPSB0aGlzLiNnZXRUeXBlT2ZBcnJheSh2YWx1ZSk7XG4gICAgICAgIGlmIChhcnJheVR5cGUgPT09IFwiT05MWV9QUklNSVRJVkVcIikge1xuICAgICAgICAgIG91dC5wdXNoKHRoaXMuI2FycmF5RGVjbGFyYXRpb24oW1xuICAgICAgICAgICAgcHJvcFxuICAgICAgICAgIF0sIHZhbHVlKSk7XG4gICAgICAgIH0gZWxzZSBpZiAoYXJyYXlUeXBlID09PSBcIk9OTFlfT0JKRUNUX0VYQ0xVRElOR19BUlJBWVwiKSB7XG4gICAgICAgICAgLy8gYXJyYXkgb2Ygb2JqZWN0c1xuICAgICAgICAgIGZvcihsZXQgaSA9IDA7IGkgPCB2YWx1ZS5sZW5ndGg7IGkrKyl7XG4gICAgICAgICAgICBvdXQucHVzaChcIlwiKTtcbiAgICAgICAgICAgIG91dC5wdXNoKHRoaXMuI2hlYWRlckdyb3VwKFtcbiAgICAgICAgICAgICAgLi4ua2V5cyxcbiAgICAgICAgICAgICAgcHJvcFxuICAgICAgICAgICAgXSkpO1xuICAgICAgICAgICAgb3V0LnB1c2goLi4udGhpcy4jcHJpbnRPYmplY3QodmFsdWVbaV0sIFtcbiAgICAgICAgICAgICAgLi4ua2V5cyxcbiAgICAgICAgICAgICAgcHJvcFxuICAgICAgICAgICAgXSkpO1xuICAgICAgICAgIH1cbiAgICAgICAgfSBlbHNlIHtcbiAgICAgICAgICAvLyB0aGlzIGlzIGEgY29tcGxleCBhcnJheSwgdXNlIHRoZSBpbmxpbmUgZm9ybWF0LlxuICAgICAgICAgIGNvbnN0IHN0ciA9IHZhbHVlLm1hcCgoeCk9PnRoaXMuI3ByaW50QXNJbmxpbmVWYWx1ZSh4KSkuam9pbihcIixcIik7XG4gICAgICAgICAgb3V0LnB1c2goYCR7dGhpcy4jZGVjbGFyYXRpb24oW1xuICAgICAgICAgICAgcHJvcFxuICAgICAgICAgIF0pfVske3N0cn1dYCk7XG4gICAgICAgIH1cbiAgICAgIH0gZWxzZSBpZiAodHlwZW9mIHZhbHVlID09PSBcIm9iamVjdFwiKSB7XG4gICAgICAgIG91dC5wdXNoKFwiXCIpO1xuICAgICAgICBvdXQucHVzaCh0aGlzLiNoZWFkZXIoW1xuICAgICAgICAgIC4uLmtleXMsXG4gICAgICAgICAgcHJvcFxuICAgICAgICBdKSk7XG4gICAgICAgIGlmICh2YWx1ZSkge1xuICAgICAgICAgIGNvbnN0IHRvUGFyc2UgPSB2YWx1ZTtcbiAgICAgICAgICBvdXQucHVzaCguLi50aGlzLiNwcmludE9iamVjdCh0b1BhcnNlLCBbXG4gICAgICAgICAgICAuLi5rZXlzLFxuICAgICAgICAgICAgcHJvcFxuICAgICAgICAgIF0pKTtcbiAgICAgICAgfVxuICAgICAgLy8gb3V0LnB1c2goLi4udGhpcy5fcGFyc2UodmFsdWUsIGAke3BhdGh9JHtwcm9wfS5gKSk7XG4gICAgICB9XG4gICAgfVxuICAgIG91dC5wdXNoKFwiXCIpO1xuICAgIHJldHVybiBvdXQ7XG4gIH1cbiAgI2lzUHJpbWl0aXZlKHZhbHVlKSB7XG4gICAgcmV0dXJuIHZhbHVlIGluc3RhbmNlb2YgRGF0ZSB8fCB2YWx1ZSBpbnN0YW5jZW9mIFJlZ0V4cCB8fCBbXG4gICAgICBcInN0cmluZ1wiLFxuICAgICAgXCJudW1iZXJcIixcbiAgICAgIFwiYm9vbGVhblwiXG4gICAgXS5pbmNsdWRlcyh0eXBlb2YgdmFsdWUpO1xuICB9XG4gICNnZXRUeXBlT2ZBcnJheShhcnIpIHtcbiAgICBpZiAodGhpcy4jYXJyYXlUeXBlQ2FjaGUuaGFzKGFycikpIHtcbiAgICAgIHJldHVybiB0aGlzLiNhcnJheVR5cGVDYWNoZS5nZXQoYXJyKTtcbiAgICB9XG4gICAgY29uc3QgdHlwZSA9IHRoaXMuI2RvR2V0VHlwZU9mQXJyYXkoYXJyKTtcbiAgICB0aGlzLiNhcnJheVR5cGVDYWNoZS5zZXQoYXJyLCB0eXBlKTtcbiAgICByZXR1cm4gdHlwZTtcbiAgfVxuICAjZG9HZXRUeXBlT2ZBcnJheShhcnIpIHtcbiAgICBpZiAoIWFyci5sZW5ndGgpIHtcbiAgICAgIC8vIGFueSB0eXBlIHNob3VsZCBiZSBmaW5lXG4gICAgICByZXR1cm4gXCJPTkxZX1BSSU1JVElWRVwiO1xuICAgIH1cbiAgICBjb25zdCBvbmx5UHJpbWl0aXZlID0gdGhpcy4jaXNQcmltaXRpdmUoYXJyWzBdKTtcbiAgICBpZiAoYXJyWzBdIGluc3RhbmNlb2YgQXJyYXkpIHtcbiAgICAgIHJldHVybiBcIk1JWEVEXCI7XG4gICAgfVxuICAgIGZvcihsZXQgaSA9IDE7IGkgPCBhcnIubGVuZ3RoOyBpKyspe1xuICAgICAgaWYgKG9ubHlQcmltaXRpdmUgIT09IHRoaXMuI2lzUHJpbWl0aXZlKGFycltpXSkgfHwgYXJyW2ldIGluc3RhbmNlb2YgQXJyYXkpIHtcbiAgICAgICAgcmV0dXJuIFwiTUlYRURcIjtcbiAgICAgIH1cbiAgICB9XG4gICAgcmV0dXJuIG9ubHlQcmltaXRpdmUgPyBcIk9OTFlfUFJJTUlUSVZFXCIgOiBcIk9OTFlfT0JKRUNUX0VYQ0xVRElOR19BUlJBWVwiO1xuICB9XG4gICNwcmludEFzSW5saW5lVmFsdWUodmFsdWUpIHtcbiAgICBpZiAodmFsdWUgaW5zdGFuY2VvZiBEYXRlKSB7XG4gICAgICByZXR1cm4gYFwiJHt0aGlzLiNwcmludERhdGUodmFsdWUpfVwiYDtcbiAgICB9IGVsc2UgaWYgKHR5cGVvZiB2YWx1ZSA9PT0gXCJzdHJpbmdcIiB8fCB2YWx1ZSBpbnN0YW5jZW9mIFJlZ0V4cCkge1xuICAgICAgcmV0dXJuIEpTT04uc3RyaW5naWZ5KHZhbHVlLnRvU3RyaW5nKCkpO1xuICAgIH0gZWxzZSBpZiAodHlwZW9mIHZhbHVlID09PSBcIm51bWJlclwiKSB7XG4gICAgICByZXR1cm4gdmFsdWU7XG4gICAgfSBlbHNlIGlmICh0eXBlb2YgdmFsdWUgPT09IFwiYm9vbGVhblwiKSB7XG4gICAgICByZXR1cm4gdmFsdWUudG9TdHJpbmcoKTtcbiAgICB9IGVsc2UgaWYgKHZhbHVlIGluc3RhbmNlb2YgQXJyYXkpIHtcbiAgICAgIGNvbnN0IHN0ciA9IHZhbHVlLm1hcCgoeCk9PnRoaXMuI3ByaW50QXNJbmxpbmVWYWx1ZSh4KSkuam9pbihcIixcIik7XG4gICAgICByZXR1cm4gYFske3N0cn1dYDtcbiAgICB9IGVsc2UgaWYgKHR5cGVvZiB2YWx1ZSA9PT0gXCJvYmplY3RcIikge1xuICAgICAgaWYgKCF2YWx1ZSkge1xuICAgICAgICB0aHJvdyBuZXcgRXJyb3IoXCJTaG91bGQgbmV2ZXIgcmVhY2hcIik7XG4gICAgICB9XG4gICAgICBjb25zdCBzdHIgPSBPYmplY3Qua2V5cyh2YWx1ZSkubWFwKChrZXkpPT57XG4gICAgICAgIHJldHVybiBgJHtqb2luS2V5cyhbXG4gICAgICAgICAga2V5XG4gICAgICAgIF0pfSA9ICR7Ly8gZGVuby1saW50LWlnbm9yZSBuby1leHBsaWNpdC1hbnlcbiAgICAgICAgdGhpcy4jcHJpbnRBc0lubGluZVZhbHVlKHZhbHVlW2tleV0pfWA7XG4gICAgICB9KS5qb2luKFwiLFwiKTtcbiAgICAgIHJldHVybiBgeyR7c3RyfX1gO1xuICAgIH1cbiAgICB0aHJvdyBuZXcgRXJyb3IoXCJTaG91bGQgbmV2ZXIgcmVhY2hcIik7XG4gIH1cbiAgI2lzU2ltcGx5U2VyaWFsaXphYmxlKHZhbHVlKSB7XG4gICAgcmV0dXJuIHR5cGVvZiB2YWx1ZSA9PT0gXCJzdHJpbmdcIiB8fCB0eXBlb2YgdmFsdWUgPT09IFwibnVtYmVyXCIgfHwgdHlwZW9mIHZhbHVlID09PSBcImJvb2xlYW5cIiB8fCB2YWx1ZSBpbnN0YW5jZW9mIFJlZ0V4cCB8fCB2YWx1ZSBpbnN0YW5jZW9mIERhdGUgfHwgdmFsdWUgaW5zdGFuY2VvZiBBcnJheSAmJiB0aGlzLiNnZXRUeXBlT2ZBcnJheSh2YWx1ZSkgIT09IFwiT05MWV9PQkpFQ1RfRVhDTFVESU5HX0FSUkFZXCI7XG4gIH1cbiAgI2hlYWRlcihrZXlzKSB7XG4gICAgcmV0dXJuIGBbJHtqb2luS2V5cyhrZXlzKX1dYDtcbiAgfVxuICAjaGVhZGVyR3JvdXAoa2V5cykge1xuICAgIHJldHVybiBgW1ske2pvaW5LZXlzKGtleXMpfV1dYDtcbiAgfVxuICAjZGVjbGFyYXRpb24oa2V5cykge1xuICAgIGNvbnN0IHRpdGxlID0gam9pbktleXMoa2V5cyk7XG4gICAgaWYgKHRpdGxlLmxlbmd0aCA+IHRoaXMubWF4UGFkKSB7XG4gICAgICB0aGlzLm1heFBhZCA9IHRpdGxlLmxlbmd0aDtcbiAgICB9XG4gICAgcmV0dXJuIGAke3RpdGxlfSA9IGA7XG4gIH1cbiAgI2FycmF5RGVjbGFyYXRpb24oa2V5cywgdmFsdWUpIHtcbiAgICByZXR1cm4gYCR7dGhpcy4jZGVjbGFyYXRpb24oa2V5cyl9JHtKU09OLnN0cmluZ2lmeSh2YWx1ZSl9YDtcbiAgfVxuICAjc3RyRGVjbGFyYXRpb24oa2V5cywgdmFsdWUpIHtcbiAgICByZXR1cm4gYCR7dGhpcy4jZGVjbGFyYXRpb24oa2V5cyl9JHtKU09OLnN0cmluZ2lmeSh2YWx1ZSl9YDtcbiAgfVxuICAjbnVtYmVyRGVjbGFyYXRpb24oa2V5cywgdmFsdWUpIHtcbiAgICBpZiAoTnVtYmVyLmlzTmFOKHZhbHVlKSkge1xuICAgICAgcmV0dXJuIGAke3RoaXMuI2RlY2xhcmF0aW9uKGtleXMpfW5hbmA7XG4gICAgfVxuICAgIHN3aXRjaCh2YWx1ZSl7XG4gICAgICBjYXNlIEluZmluaXR5OlxuICAgICAgICByZXR1cm4gYCR7dGhpcy4jZGVjbGFyYXRpb24oa2V5cyl9aW5mYDtcbiAgICAgIGNhc2UgLUluZmluaXR5OlxuICAgICAgICByZXR1cm4gYCR7dGhpcy4jZGVjbGFyYXRpb24oa2V5cyl9LWluZmA7XG4gICAgICBkZWZhdWx0OlxuICAgICAgICByZXR1cm4gYCR7dGhpcy4jZGVjbGFyYXRpb24oa2V5cyl9JHt2YWx1ZX1gO1xuICAgIH1cbiAgfVxuICAjYm9vbERlY2xhcmF0aW9uKGtleXMsIHZhbHVlKSB7XG4gICAgcmV0dXJuIGAke3RoaXMuI2RlY2xhcmF0aW9uKGtleXMpfSR7dmFsdWV9YDtcbiAgfVxuICAjcHJpbnREYXRlKHZhbHVlKSB7XG4gICAgZnVuY3Rpb24gZHRQYWQodiwgbFBhZCA9IDIpIHtcbiAgICAgIHJldHVybiB2LnBhZFN0YXJ0KGxQYWQsIFwiMFwiKTtcbiAgICB9XG4gICAgY29uc3QgbSA9IGR0UGFkKCh2YWx1ZS5nZXRVVENNb250aCgpICsgMSkudG9TdHJpbmcoKSk7XG4gICAgY29uc3QgZCA9IGR0UGFkKHZhbHVlLmdldFVUQ0RhdGUoKS50b1N0cmluZygpKTtcbiAgICBjb25zdCBoID0gZHRQYWQodmFsdWUuZ2V0VVRDSG91cnMoKS50b1N0cmluZygpKTtcbiAgICBjb25zdCBtaW4gPSBkdFBhZCh2YWx1ZS5nZXRVVENNaW51dGVzKCkudG9TdHJpbmcoKSk7XG4gICAgY29uc3QgcyA9IGR0UGFkKHZhbHVlLmdldFVUQ1NlY29uZHMoKS50b1N0cmluZygpKTtcbiAgICBjb25zdCBtcyA9IGR0UGFkKHZhbHVlLmdldFVUQ01pbGxpc2Vjb25kcygpLnRvU3RyaW5nKCksIDMpO1xuICAgIC8vIGZvcm1hdHRlZCBkYXRlXG4gICAgY29uc3QgZkRhdGEgPSBgJHt2YWx1ZS5nZXRVVENGdWxsWWVhcigpfS0ke219LSR7ZH1UJHtofToke21pbn06JHtzfS4ke21zfWA7XG4gICAgcmV0dXJuIGZEYXRhO1xuICB9XG4gICNkYXRlRGVjbGFyYXRpb24oa2V5cywgdmFsdWUpIHtcbiAgICByZXR1cm4gYCR7dGhpcy4jZGVjbGFyYXRpb24oa2V5cyl9JHt0aGlzLiNwcmludERhdGUodmFsdWUpfWA7XG4gIH1cbiAgI2Zvcm1hdChvcHRpb25zID0ge30pIHtcbiAgICBjb25zdCB7IGtleUFsaWdubWVudCA9IGZhbHNlIH0gPSBvcHRpb25zO1xuICAgIGNvbnN0IHJEZWNsYXJhdGlvbiA9IC9eKFxcXCIuKlxcXCJ8W149XSopXFxzPS87XG4gICAgY29uc3Qgb3V0ID0gW107XG4gICAgZm9yKGxldCBpID0gMDsgaSA8IHRoaXMub3V0cHV0Lmxlbmd0aDsgaSsrKXtcbiAgICAgIGNvbnN0IGwgPSB0aGlzLm91dHB1dFtpXTtcbiAgICAgIC8vIHdlIGtlZXAgZW1wdHkgZW50cnkgZm9yIGFycmF5IG9mIG9iamVjdHNcbiAgICAgIGlmIChsWzBdID09PSBcIltcIiAmJiBsWzFdICE9PSBcIltcIikge1xuICAgICAgICAvLyBub24tZW1wdHkgb2JqZWN0IHdpdGggb25seSBzdWJvYmplY3RzIGFzIHByb3BlcnRpZXNcbiAgICAgICAgaWYgKHRoaXMub3V0cHV0W2kgKyAxXSA9PT0gXCJcIiAmJiB0aGlzLm91dHB1dFtpICsgMl0/LnNsaWNlKDAsIGwubGVuZ3RoKSA9PT0gbC5zbGljZSgwLCAtMSkgKyBcIi5cIikge1xuICAgICAgICAgIGkgKz0gMTtcbiAgICAgICAgICBjb250aW51ZTtcbiAgICAgICAgfVxuICAgICAgICBvdXQucHVzaChsKTtcbiAgICAgIH0gZWxzZSB7XG4gICAgICAgIGlmIChrZXlBbGlnbm1lbnQpIHtcbiAgICAgICAgICBjb25zdCBtID0gckRlY2xhcmF0aW9uLmV4ZWMobCk7XG4gICAgICAgICAgaWYgKG0gJiYgbVsxXSkge1xuICAgICAgICAgICAgb3V0LnB1c2gobC5yZXBsYWNlKG1bMV0sIG1bMV0ucGFkRW5kKHRoaXMubWF4UGFkKSkpO1xuICAgICAgICAgIH0gZWxzZSB7XG4gICAgICAgICAgICBvdXQucHVzaChsKTtcbiAgICAgICAgICB9XG4gICAgICAgIH0gZWxzZSB7XG4gICAgICAgICAgb3V0LnB1c2gobCk7XG4gICAgICAgIH1cbiAgICAgIH1cbiAgICB9XG4gICAgLy8gQ2xlYW5pbmcgbXVsdGlwbGUgc3BhY2VzXG4gICAgY29uc3QgY2xlYW5lZE91dHB1dCA9IFtdO1xuICAgIGZvcihsZXQgaSA9IDA7IGkgPCBvdXQubGVuZ3RoOyBpKyspe1xuICAgICAgY29uc3QgbCA9IG91dFtpXTtcbiAgICAgIGlmICghKGwgPT09IFwiXCIgJiYgb3V0W2kgKyAxXSA9PT0gXCJcIikpIHtcbiAgICAgICAgY2xlYW5lZE91dHB1dC5wdXNoKGwpO1xuICAgICAgfVxuICAgIH1cbiAgICByZXR1cm4gY2xlYW5lZE91dHB1dDtcbiAgfVxufVxuLyoqXG4gKiBDb252ZXJ0cyBhbiBvYmplY3QgdG8gYSB7QGxpbmsgaHR0cHM6Ly90b21sLmlvIHwgVE9NTH0gc3RyaW5nLlxuICpcbiAqIEBleGFtcGxlIFVzYWdlXG4gKiBgYGB0c1xuICogaW1wb3J0IHsgc3RyaW5naWZ5IH0gZnJvbSBcIkBzdGQvdG9tbC9zdHJpbmdpZnlcIjtcbiAqIGltcG9ydCB7IGFzc2VydEVxdWFscyB9IGZyb20gXCJAc3RkL2Fzc2VydFwiO1xuICpcbiAqIGNvbnN0IG9iaiA9IHtcbiAqICAgdGl0bGU6IFwiVE9NTCBFeGFtcGxlXCIsXG4gKiAgIG93bmVyOiB7XG4gKiAgICAgbmFtZTogXCJCb2JcIixcbiAqICAgICBiaW86IFwiQm9iIGlzIGEgY29vbCBndXlcIixcbiAqICB9XG4gKiB9O1xuICogY29uc3QgdG9tbFN0cmluZyA9IHN0cmluZ2lmeShvYmopO1xuICogYXNzZXJ0RXF1YWxzKHRvbWxTdHJpbmcsIGB0aXRsZSA9IFwiVE9NTCBFeGFtcGxlXCJcXG5cXG5bb3duZXJdXFxubmFtZSA9IFwiQm9iXCJcXG5iaW8gPSBcIkJvYiBpcyBhIGNvb2wgZ3V5XCJcXG5gKTtcbiAqIGBgYFxuICogQHBhcmFtIG9iaiBTb3VyY2Ugb2JqZWN0XG4gKiBAcGFyYW0gb3B0aW9ucyBPcHRpb25zIGZvciBzdHJpbmdpZnlpbmcuXG4gKiBAcmV0dXJucyBUT01MIHN0cmluZ1xuICovIGV4cG9ydCBmdW5jdGlvbiBzdHJpbmdpZnkob2JqLCBvcHRpb25zKSB7XG4gIHJldHVybiBuZXcgRHVtcGVyKG9iaikuZHVtcChvcHRpb25zKS5qb2luKFwiXFxuXCIpO1xufVxuLy8jIHNvdXJjZU1hcHBpbmdVUkw9c3RyaW5naWZ5LmpzLm1hcCIsIi8vIENvcHlyaWdodCAyMDE4LTIwMjUgdGhlIERlbm8gYXV0aG9ycy4gTUlUIGxpY2Vuc2UuXG4vLyBUaGlzIG1vZHVsZSBpcyBicm93c2VyIGNvbXBhdGlibGUuXG4vKipcbiAqIEZpbHRlcnMgdGhlIGdpdmVuIGFycmF5LCByZW1vdmluZyBhbGwgZWxlbWVudHMgdGhhdCBkbyBub3QgbWF0Y2ggdGhlIGdpdmVuIHByZWRpY2F0ZVxuICogKippbiBwbGFjZS4gVGhpcyBtZWFucyBgYXJyYXlgIHdpbGwgYmUgbW9kaWZpZWQhKiouXG4gKi8gZXhwb3J0IGZ1bmN0aW9uIGZpbHRlckluUGxhY2UoYXJyYXksIHByZWRpY2F0ZSkge1xuICBsZXQgb3V0cHV0SW5kZXggPSAwO1xuICBmb3IgKGNvbnN0IGN1ciBvZiBhcnJheSl7XG4gICAgaWYgKCFwcmVkaWNhdGUoY3VyKSkge1xuICAgICAgY29udGludWU7XG4gICAgfVxuICAgIGFycmF5W291dHB1dEluZGV4XSA9IGN1cjtcbiAgICBvdXRwdXRJbmRleCArPSAxO1xuICB9XG4gIGFycmF5LnNwbGljZShvdXRwdXRJbmRleCk7XG4gIHJldHVybiBhcnJheTtcbn1cbi8vIyBzb3VyY2VNYXBwaW5nVVJMPV91dGlscy5qcy5tYXAiLCIvLyBDb3B5cmlnaHQgMjAxOC0yMDI1IHRoZSBEZW5vIGF1dGhvcnMuIE1JVCBsaWNlbnNlLlxuLy8gVGhpcyBtb2R1bGUgaXMgYnJvd3NlciBjb21wYXRpYmxlLlxuaW1wb3J0IHsgZmlsdGVySW5QbGFjZSB9IGZyb20gXCIuL191dGlscy5qc1wiO1xuZXhwb3J0IGZ1bmN0aW9uIGRlZXBNZXJnZShyZWNvcmQsIG90aGVyLCBvcHRpb25zKSB7XG4gIHJldHVybiBkZWVwTWVyZ2VJbnRlcm5hbChyZWNvcmQsIG90aGVyLCBuZXcgU2V0KCksIG9wdGlvbnMpO1xufVxuZnVuY3Rpb24gZGVlcE1lcmdlSW50ZXJuYWwocmVjb3JkLCBvdGhlciwgc2Vlbiwgb3B0aW9ucykge1xuICBjb25zdCByZXN1bHQgPSB7fTtcbiAgY29uc3Qga2V5cyA9IG5ldyBTZXQoW1xuICAgIC4uLmdldEtleXMocmVjb3JkKSxcbiAgICAuLi5nZXRLZXlzKG90aGVyKVxuICBdKTtcbiAgLy8gSXRlcmF0ZSB0aHJvdWdoIGVhY2gga2V5IG9mIG90aGVyIG9iamVjdCBhbmQgdXNlIGNvcnJlY3QgbWVyZ2luZyBzdHJhdGVneVxuICBmb3IgKGNvbnN0IGtleSBvZiBrZXlzKXtcbiAgICAvLyBTa2lwIHRvIHByZXZlbnQgT2JqZWN0LnByb3RvdHlwZS5fX3Byb3RvX18gYWNjZXNzb3IgcHJvcGVydHkgY2FsbHMgb24gbm9uLURlbm8gcGxhdGZvcm1zXG4gICAgaWYgKGtleSA9PT0gXCJfX3Byb3RvX19cIikge1xuICAgICAgY29udGludWU7XG4gICAgfVxuICAgIGNvbnN0IGEgPSByZWNvcmRba2V5XTtcbiAgICBpZiAoIU9iamVjdC5oYXNPd24ob3RoZXIsIGtleSkpIHtcbiAgICAgIHJlc3VsdFtrZXldID0gYTtcbiAgICAgIGNvbnRpbnVlO1xuICAgIH1cbiAgICBjb25zdCBiID0gb3RoZXJba2V5XTtcbiAgICBpZiAoaXNOb25OdWxsT2JqZWN0KGEpICYmIGlzTm9uTnVsbE9iamVjdChiKSAmJiAhc2Vlbi5oYXMoYSkgJiYgIXNlZW4uaGFzKGIpKSB7XG4gICAgICBzZWVuLmFkZChhKTtcbiAgICAgIHNlZW4uYWRkKGIpO1xuICAgICAgcmVzdWx0W2tleV0gPSBtZXJnZU9iamVjdHMoYSwgYiwgc2Vlbiwgb3B0aW9ucyk7XG4gICAgICBjb250aW51ZTtcbiAgICB9XG4gICAgLy8gT3ZlcnJpZGUgdmFsdWVcbiAgICByZXN1bHRba2V5XSA9IGI7XG4gIH1cbiAgcmV0dXJuIHJlc3VsdDtcbn1cbmZ1bmN0aW9uIG1lcmdlT2JqZWN0cyhsZWZ0LCByaWdodCwgc2Vlbiwgb3B0aW9ucyA9IHtcbiAgYXJyYXlzOiBcIm1lcmdlXCIsXG4gIHNldHM6IFwibWVyZ2VcIixcbiAgbWFwczogXCJtZXJnZVwiXG59KSB7XG4gIC8vIFJlY3Vyc2l2ZWx5IG1lcmdlIG1lcmdlYWJsZSBvYmplY3RzXG4gIGlmIChpc01lcmdlYWJsZShsZWZ0KSAmJiBpc01lcmdlYWJsZShyaWdodCkpIHtcbiAgICByZXR1cm4gZGVlcE1lcmdlSW50ZXJuYWwobGVmdCwgcmlnaHQsIHNlZW4sIG9wdGlvbnMpO1xuICB9XG4gIGlmIChpc0l0ZXJhYmxlKGxlZnQpICYmIGlzSXRlcmFibGUocmlnaHQpKSB7XG4gICAgLy8gSGFuZGxlIGFycmF5c1xuICAgIGlmIChBcnJheS5pc0FycmF5KGxlZnQpICYmIEFycmF5LmlzQXJyYXkocmlnaHQpKSB7XG4gICAgICBpZiAob3B0aW9ucy5hcnJheXMgPT09IFwibWVyZ2VcIikge1xuICAgICAgICByZXR1cm4gbGVmdC5jb25jYXQocmlnaHQpO1xuICAgICAgfVxuICAgICAgcmV0dXJuIHJpZ2h0O1xuICAgIH1cbiAgICAvLyBIYW5kbGUgbWFwc1xuICAgIGlmIChsZWZ0IGluc3RhbmNlb2YgTWFwICYmIHJpZ2h0IGluc3RhbmNlb2YgTWFwKSB7XG4gICAgICBpZiAob3B0aW9ucy5tYXBzID09PSBcIm1lcmdlXCIpIHtcbiAgICAgICAgcmV0dXJuIG5ldyBNYXAoW1xuICAgICAgICAgIC4uLmxlZnQsXG4gICAgICAgICAgLi4ucmlnaHRcbiAgICAgICAgXSk7XG4gICAgICB9XG4gICAgICByZXR1cm4gcmlnaHQ7XG4gICAgfVxuICAgIC8vIEhhbmRsZSBzZXRzXG4gICAgaWYgKGxlZnQgaW5zdGFuY2VvZiBTZXQgJiYgcmlnaHQgaW5zdGFuY2VvZiBTZXQpIHtcbiAgICAgIGlmIChvcHRpb25zLnNldHMgPT09IFwibWVyZ2VcIikge1xuICAgICAgICByZXR1cm4gbmV3IFNldChbXG4gICAgICAgICAgLi4ubGVmdCxcbiAgICAgICAgICAuLi5yaWdodFxuICAgICAgICBdKTtcbiAgICAgIH1cbiAgICAgIHJldHVybiByaWdodDtcbiAgICB9XG4gIH1cbiAgcmV0dXJuIHJpZ2h0O1xufVxuLyoqXG4gKiBUZXN0IHdoZXRoZXIgYSB2YWx1ZSBpcyBtZXJnZWFibGUgb3Igbm90XG4gKiBCdWlsdGlucyB0aGF0IGxvb2sgbGlrZSBvYmplY3RzLCBudWxsIGFuZCB1c2VyIGRlZmluZWQgY2xhc3Nlc1xuICogYXJlIG5vdCBjb25zaWRlcmVkIG1lcmdlYWJsZSAoaXQgbWVhbnMgdGhhdCByZWZlcmVuY2Ugd2lsbCBiZSBjb3BpZWQpXG4gKi8gZnVuY3Rpb24gaXNNZXJnZWFibGUodmFsdWUpIHtcbiAgcmV0dXJuIE9iamVjdC5nZXRQcm90b3R5cGVPZih2YWx1ZSkgPT09IE9iamVjdC5wcm90b3R5cGU7XG59XG5mdW5jdGlvbiBpc0l0ZXJhYmxlKHZhbHVlKSB7XG4gIHJldHVybiB0eXBlb2YgdmFsdWVbU3ltYm9sLml0ZXJhdG9yXSA9PT0gXCJmdW5jdGlvblwiO1xufVxuZnVuY3Rpb24gaXNOb25OdWxsT2JqZWN0KHZhbHVlKSB7XG4gIHJldHVybiB2YWx1ZSAhPT0gbnVsbCAmJiB0eXBlb2YgdmFsdWUgPT09IFwib2JqZWN0XCI7XG59XG5mdW5jdGlvbiBnZXRLZXlzKHJlY29yZCkge1xuICBjb25zdCByZXN1bHQgPSBPYmplY3QuZ2V0T3duUHJvcGVydHlTeW1ib2xzKHJlY29yZCk7XG4gIGZpbHRlckluUGxhY2UocmVzdWx0LCAoa2V5KT0+T2JqZWN0LnByb3RvdHlwZS5wcm9wZXJ0eUlzRW51bWVyYWJsZS5jYWxsKHJlY29yZCwga2V5KSk7XG4gIHJlc3VsdC5wdXNoKC4uLk9iamVjdC5rZXlzKHJlY29yZCkpO1xuICByZXR1cm4gcmVzdWx0O1xufVxuLy8jIHNvdXJjZU1hcHBpbmdVUkw9ZGVlcF9tZXJnZS5qcy5tYXAiLCIvLyBDb3B5cmlnaHQgMjAxOC0yMDI1IHRoZSBEZW5vIGF1dGhvcnMuIE1JVCBsaWNlbnNlLlxuLy8gVGhpcyBtb2R1bGUgaXMgYnJvd3NlciBjb21wYXRpYmxlLlxuaW1wb3J0IHsgZGVlcE1lcmdlIH0gZnJvbSBcIkBqc3Ivc3RkX19jb2xsZWN0aW9ucy9kZWVwLW1lcmdlXCI7XG4vKipcbiAqIENvcHkgb2YgYGltcG9ydCB7IGlzTGVhcCB9IGZyb20gXCJAc3RkL2RhdGV0aW1lXCI7YCBiZWNhdXNlIGl0IGNhbm5vdCBiZSBpbXBvdGVkIGFzIGxvbmcgYXMgaXQgaXMgdW5zdGFibGUuXG4gKi8gZnVuY3Rpb24gaXNMZWFwKHllYXJOdW1iZXIpIHtcbiAgcmV0dXJuIHllYXJOdW1iZXIgJSA0ID09PSAwICYmIHllYXJOdW1iZXIgJSAxMDAgIT09IDAgfHwgeWVhck51bWJlciAlIDQwMCA9PT0gMDtcbn1cbmV4cG9ydCBjbGFzcyBTY2FubmVyIHtcbiAgI3doaXRlc3BhY2UgPSAvWyBcXHRdLztcbiAgI3Bvc2l0aW9uID0gMDtcbiAgI3NvdXJjZTtcbiAgY29uc3RydWN0b3Ioc291cmNlKXtcbiAgICB0aGlzLiNzb3VyY2UgPSBzb3VyY2U7XG4gIH1cbiAgZ2V0IHBvc2l0aW9uKCkge1xuICAgIHJldHVybiB0aGlzLiNwb3NpdGlvbjtcbiAgfVxuICBnZXQgc291cmNlKCkge1xuICAgIHJldHVybiB0aGlzLiNzb3VyY2U7XG4gIH1cbiAgLyoqXG4gICAqIEdldCBjdXJyZW50IGNoYXJhY3RlclxuICAgKiBAcGFyYW0gaW5kZXggLSByZWxhdGl2ZSBpbmRleCBmcm9tIGN1cnJlbnQgcG9zaXRpb25cbiAgICovIGNoYXIoaW5kZXggPSAwKSB7XG4gICAgcmV0dXJuIHRoaXMuI3NvdXJjZVt0aGlzLiNwb3NpdGlvbiArIGluZGV4XSA/PyBcIlwiO1xuICB9XG4gIC8qKlxuICAgKiBHZXQgc2xpY2VkIHN0cmluZ1xuICAgKiBAcGFyYW0gc3RhcnQgLSBzdGFydCBwb3NpdGlvbiByZWxhdGl2ZSBmcm9tIGN1cnJlbnQgcG9zaXRpb25cbiAgICogQHBhcmFtIGVuZCAtIGVuZCBwb3NpdGlvbiByZWxhdGl2ZSBmcm9tIGN1cnJlbnQgcG9zaXRpb25cbiAgICovIHNsaWNlKHN0YXJ0LCBlbmQpIHtcbiAgICByZXR1cm4gdGhpcy4jc291cmNlLnNsaWNlKHRoaXMuI3Bvc2l0aW9uICsgc3RhcnQsIHRoaXMuI3Bvc2l0aW9uICsgZW5kKTtcbiAgfVxuICAvKipcbiAgICogTW92ZSBwb3NpdGlvbiB0byBuZXh0XG4gICAqLyBuZXh0KGNvdW50ID0gMSkge1xuICAgIHRoaXMuI3Bvc2l0aW9uICs9IGNvdW50O1xuICB9XG4gIHNraXBXaGl0ZXNwYWNlcygpIHtcbiAgICB3aGlsZSh0aGlzLiN3aGl0ZXNwYWNlLnRlc3QodGhpcy5jaGFyKCkpICYmICF0aGlzLmVvZigpKXtcbiAgICAgIHRoaXMubmV4dCgpO1xuICAgIH1cbiAgICAvLyBJbnZhbGlkIGlmIGN1cnJlbnQgY2hhciBpcyBvdGhlciBraW5kcyBvZiB3aGl0ZXNwYWNlXG4gICAgaWYgKCF0aGlzLmlzQ3VycmVudENoYXJFT0woKSAmJiAvXFxzLy50ZXN0KHRoaXMuY2hhcigpKSkge1xuICAgICAgY29uc3QgZXNjYXBlZCA9IFwiXFxcXHVcIiArIHRoaXMuY2hhcigpLmNoYXJDb2RlQXQoMCkudG9TdHJpbmcoMTYpO1xuICAgICAgY29uc3QgcG9zaXRpb24gPSB0aGlzLiNwb3NpdGlvbjtcbiAgICAgIHRocm93IG5ldyBTeW50YXhFcnJvcihgQ2Fubm90IHBhcnNlIHRoZSBUT01MOiBJdCBjb250YWlucyBpbnZhbGlkIHdoaXRlc3BhY2UgYXQgcG9zaXRpb24gJyR7cG9zaXRpb259JzogXFxgJHtlc2NhcGVkfVxcYGApO1xuICAgIH1cbiAgfVxuICBuZXh0VW50aWxDaGFyKG9wdGlvbnMgPSB7XG4gICAgc2tpcENvbW1lbnRzOiB0cnVlXG4gIH0pIHtcbiAgICB3aGlsZSghdGhpcy5lb2YoKSl7XG4gICAgICBjb25zdCBjaGFyID0gdGhpcy5jaGFyKCk7XG4gICAgICBpZiAodGhpcy4jd2hpdGVzcGFjZS50ZXN0KGNoYXIpIHx8IHRoaXMuaXNDdXJyZW50Q2hhckVPTCgpKSB7XG4gICAgICAgIHRoaXMubmV4dCgpO1xuICAgICAgfSBlbHNlIGlmIChvcHRpb25zLnNraXBDb21tZW50cyAmJiB0aGlzLmNoYXIoKSA9PT0gXCIjXCIpIHtcbiAgICAgICAgLy8gZW50ZXJpbmcgY29tbWVudFxuICAgICAgICB3aGlsZSghdGhpcy5pc0N1cnJlbnRDaGFyRU9MKCkgJiYgIXRoaXMuZW9mKCkpe1xuICAgICAgICAgIHRoaXMubmV4dCgpO1xuICAgICAgICB9XG4gICAgICB9IGVsc2Uge1xuICAgICAgICBicmVhaztcbiAgICAgIH1cbiAgICB9XG4gIH1cbiAgLyoqXG4gICAqIFBvc2l0aW9uIHJlYWNoZWQgRU9GIG9yIG5vdFxuICAgKi8gZW9mKCkge1xuICAgIHJldHVybiB0aGlzLiNwb3NpdGlvbiA+PSB0aGlzLiNzb3VyY2UubGVuZ3RoO1xuICB9XG4gIGlzQ3VycmVudENoYXJFT0woKSB7XG4gICAgcmV0dXJuIHRoaXMuY2hhcigpID09PSBcIlxcblwiIHx8IHRoaXMuc3RhcnRzV2l0aChcIlxcclxcblwiKTtcbiAgfVxuICBzdGFydHNXaXRoKHNlYXJjaFN0cmluZykge1xuICAgIHJldHVybiB0aGlzLiNzb3VyY2Uuc3RhcnRzV2l0aChzZWFyY2hTdHJpbmcsIHRoaXMuI3Bvc2l0aW9uKTtcbiAgfVxuICBtYXRjaChyZWdFeHApIHtcbiAgICBpZiAoIXJlZ0V4cC5zdGlja3kpIHtcbiAgICAgIHRocm93IG5ldyBFcnJvcihgUmVnRXhwICR7cmVnRXhwfSBkb2VzIG5vdCBoYXZlIGEgc3RpY2t5ICd5JyBmbGFnYCk7XG4gICAgfVxuICAgIHJlZ0V4cC5sYXN0SW5kZXggPSB0aGlzLiNwb3NpdGlvbjtcbiAgICByZXR1cm4gdGhpcy4jc291cmNlLm1hdGNoKHJlZ0V4cCk7XG4gIH1cbn1cbi8vIC0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tXG4vLyBVdGlsaXRpZXNcbi8vIC0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tXG5mdW5jdGlvbiBzdWNjZXNzKGJvZHkpIHtcbiAgcmV0dXJuIHtcbiAgICBvazogdHJ1ZSxcbiAgICBib2R5XG4gIH07XG59XG5mdW5jdGlvbiBmYWlsdXJlKCkge1xuICByZXR1cm4ge1xuICAgIG9rOiBmYWxzZVxuICB9O1xufVxuLyoqXG4gKiBDcmVhdGVzIGEgbmVzdGVkIG9iamVjdCBmcm9tIHRoZSBrZXlzIGFuZCB2YWx1ZXMuXG4gKlxuICogZS5nLiBgdW5mbGF0KFtcImFcIiwgXCJiXCIsIFwiY1wiXSwgMSlgIHJldHVybnMgYHsgYTogeyBiOiB7IGM6IDEgfSB9IH1gXG4gKi8gZXhwb3J0IGZ1bmN0aW9uIHVuZmxhdChrZXlzLCB2YWx1ZXMgPSB7XG4gIF9fcHJvdG9fXzogbnVsbFxufSkge1xuICByZXR1cm4ga2V5cy5yZWR1Y2VSaWdodCgoYWNjLCBrZXkpPT4oe1xuICAgICAgW2tleV06IGFjY1xuICAgIH0pLCB2YWx1ZXMpO1xufVxuZnVuY3Rpb24gaXNPYmplY3QodmFsdWUpIHtcbiAgcmV0dXJuIHR5cGVvZiB2YWx1ZSA9PT0gXCJvYmplY3RcIiAmJiB2YWx1ZSAhPT0gbnVsbDtcbn1cbmZ1bmN0aW9uIGdldFRhcmdldFZhbHVlKHRhcmdldCwga2V5cykge1xuICBjb25zdCBrZXkgPSBrZXlzWzBdO1xuICBpZiAoIWtleSkge1xuICAgIHRocm93IG5ldyBFcnJvcihcIkNhbm5vdCBwYXJzZSB0aGUgVE9NTDoga2V5IGxlbmd0aCBpcyBub3QgYSBwb3NpdGl2ZSBudW1iZXJcIik7XG4gIH1cbiAgcmV0dXJuIHRhcmdldFtrZXldO1xufVxuZnVuY3Rpb24gZGVlcEFzc2lnblRhYmxlKHRhcmdldCwgdGFibGUpIHtcbiAgY29uc3QgeyBrZXlzLCB0eXBlLCB2YWx1ZSB9ID0gdGFibGU7XG4gIGNvbnN0IGN1cnJlbnRWYWx1ZSA9IGdldFRhcmdldFZhbHVlKHRhcmdldCwga2V5cyk7XG4gIGlmIChjdXJyZW50VmFsdWUgPT09IHVuZGVmaW5lZCkge1xuICAgIHJldHVybiBPYmplY3QuYXNzaWduKHRhcmdldCwgdW5mbGF0KGtleXMsIHZhbHVlKSk7XG4gIH1cbiAgaWYgKEFycmF5LmlzQXJyYXkoY3VycmVudFZhbHVlKSkge1xuICAgIGNvbnN0IGxhc3QgPSBjdXJyZW50VmFsdWUuYXQoLTEpO1xuICAgIGRlZXBBc3NpZ24obGFzdCwge1xuICAgICAgdHlwZSxcbiAgICAgIGtleXM6IGtleXMuc2xpY2UoMSksXG4gICAgICB2YWx1ZVxuICAgIH0pO1xuICAgIHJldHVybiB0YXJnZXQ7XG4gIH1cbiAgaWYgKGlzT2JqZWN0KGN1cnJlbnRWYWx1ZSkpIHtcbiAgICBkZWVwQXNzaWduKGN1cnJlbnRWYWx1ZSwge1xuICAgICAgdHlwZSxcbiAgICAgIGtleXM6IGtleXMuc2xpY2UoMSksXG4gICAgICB2YWx1ZVxuICAgIH0pO1xuICAgIHJldHVybiB0YXJnZXQ7XG4gIH1cbiAgdGhyb3cgbmV3IEVycm9yKFwiVW5leHBlY3RlZCBhc3NpZ25cIik7XG59XG5mdW5jdGlvbiBkZWVwQXNzaWduVGFibGVBcnJheSh0YXJnZXQsIHRhYmxlKSB7XG4gIGNvbnN0IHsgdHlwZSwga2V5cywgdmFsdWUgfSA9IHRhYmxlO1xuICBjb25zdCBjdXJyZW50VmFsdWUgPSBnZXRUYXJnZXRWYWx1ZSh0YXJnZXQsIGtleXMpO1xuICBpZiAoY3VycmVudFZhbHVlID09PSB1bmRlZmluZWQpIHtcbiAgICByZXR1cm4gT2JqZWN0LmFzc2lnbih0YXJnZXQsIHVuZmxhdChrZXlzLCBbXG4gICAgICB2YWx1ZVxuICAgIF0pKTtcbiAgfVxuICBpZiAoQXJyYXkuaXNBcnJheShjdXJyZW50VmFsdWUpKSB7XG4gICAgaWYgKHRhYmxlLmtleXMubGVuZ3RoID09PSAxKSB7XG4gICAgICBjdXJyZW50VmFsdWUucHVzaCh2YWx1ZSk7XG4gICAgfSBlbHNlIHtcbiAgICAgIGNvbnN0IGxhc3QgPSBjdXJyZW50VmFsdWUuYXQoLTEpO1xuICAgICAgZGVlcEFzc2lnbihsYXN0LCB7XG4gICAgICAgIHR5cGU6IHRhYmxlLnR5cGUsXG4gICAgICAgIGtleXM6IHRhYmxlLmtleXMuc2xpY2UoMSksXG4gICAgICAgIHZhbHVlOiB0YWJsZS52YWx1ZVxuICAgICAgfSk7XG4gICAgfVxuICAgIHJldHVybiB0YXJnZXQ7XG4gIH1cbiAgaWYgKGlzT2JqZWN0KGN1cnJlbnRWYWx1ZSkpIHtcbiAgICBkZWVwQXNzaWduKGN1cnJlbnRWYWx1ZSwge1xuICAgICAgdHlwZSxcbiAgICAgIGtleXM6IGtleXMuc2xpY2UoMSksXG4gICAgICB2YWx1ZVxuICAgIH0pO1xuICAgIHJldHVybiB0YXJnZXQ7XG4gIH1cbiAgdGhyb3cgbmV3IEVycm9yKFwiVW5leHBlY3RlZCBhc3NpZ25cIik7XG59XG5leHBvcnQgZnVuY3Rpb24gZGVlcEFzc2lnbih0YXJnZXQsIGJvZHkpIHtcbiAgc3dpdGNoKGJvZHkudHlwZSl7XG4gICAgY2FzZSBcIkJsb2NrXCI6XG4gICAgICByZXR1cm4gZGVlcE1lcmdlKHRhcmdldCwgYm9keS52YWx1ZSk7XG4gICAgY2FzZSBcIlRhYmxlXCI6XG4gICAgICByZXR1cm4gZGVlcEFzc2lnblRhYmxlKHRhcmdldCwgYm9keSk7XG4gICAgY2FzZSBcIlRhYmxlQXJyYXlcIjpcbiAgICAgIHJldHVybiBkZWVwQXNzaWduVGFibGVBcnJheSh0YXJnZXQsIGJvZHkpO1xuICB9XG59XG4vLyAtLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS1cbi8vIFBhcnNlciBjb21iaW5hdG9ycyBhbmQgZ2VuZXJhdG9yc1xuLy8gLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tXG4vLyBkZW5vLWxpbnQtaWdub3JlIG5vLWV4cGxpY2l0LWFueVxuZnVuY3Rpb24gb3IocGFyc2Vycykge1xuICByZXR1cm4gKHNjYW5uZXIpPT57XG4gICAgZm9yIChjb25zdCBwYXJzZSBvZiBwYXJzZXJzKXtcbiAgICAgIGNvbnN0IHJlc3VsdCA9IHBhcnNlKHNjYW5uZXIpO1xuICAgICAgaWYgKHJlc3VsdC5vaykgcmV0dXJuIHJlc3VsdDtcbiAgICB9XG4gICAgcmV0dXJuIGZhaWx1cmUoKTtcbiAgfTtcbn1cbi8qKiBKb2luIHRoZSBwYXJzZSByZXN1bHRzIG9mIHRoZSBnaXZlbiBwYXJzZXIgaW50byBhbiBhcnJheS5cbiAqXG4gKiBJZiB0aGUgcGFyc2VyIGZhaWxzIGF0IHRoZSBmaXJzdCBhdHRlbXB0LCBpdCB3aWxsIHJldHVybiBhbiBlbXB0eSBhcnJheS5cbiAqLyBmdW5jdGlvbiBqb2luKHBhcnNlciwgc2VwYXJhdG9yKSB7XG4gIGNvbnN0IFNlcGFyYXRvciA9IGNoYXJhY3RlcihzZXBhcmF0b3IpO1xuICByZXR1cm4gKHNjYW5uZXIpPT57XG4gICAgY29uc3Qgb3V0ID0gW107XG4gICAgY29uc3QgZmlyc3QgPSBwYXJzZXIoc2Nhbm5lcik7XG4gICAgaWYgKCFmaXJzdC5vaykgcmV0dXJuIHN1Y2Nlc3Mob3V0KTtcbiAgICBvdXQucHVzaChmaXJzdC5ib2R5KTtcbiAgICB3aGlsZSghc2Nhbm5lci5lb2YoKSl7XG4gICAgICBpZiAoIVNlcGFyYXRvcihzY2FubmVyKS5vaykgYnJlYWs7XG4gICAgICBjb25zdCByZXN1bHQgPSBwYXJzZXIoc2Nhbm5lcik7XG4gICAgICBpZiAoIXJlc3VsdC5vaykge1xuICAgICAgICB0aHJvdyBuZXcgU3ludGF4RXJyb3IoYEludmFsaWQgdG9rZW4gYWZ0ZXIgXCIke3NlcGFyYXRvcn1cImApO1xuICAgICAgfVxuICAgICAgb3V0LnB1c2gocmVzdWx0LmJvZHkpO1xuICAgIH1cbiAgICByZXR1cm4gc3VjY2VzcyhvdXQpO1xuICB9O1xufVxuLyoqIEpvaW4gdGhlIHBhcnNlIHJlc3VsdHMgb2YgdGhlIGdpdmVuIHBhcnNlciBpbnRvIGFuIGFycmF5LlxuICpcbiAqIFRoaXMgcmVxdWlyZXMgdGhlIHBhcnNlciB0byBzdWNjZWVkIGF0IGxlYXN0IG9uY2UuXG4gKi8gZnVuY3Rpb24gam9pbjEocGFyc2VyLCBzZXBhcmF0b3IpIHtcbiAgY29uc3QgU2VwYXJhdG9yID0gY2hhcmFjdGVyKHNlcGFyYXRvcik7XG4gIHJldHVybiAoc2Nhbm5lcik9PntcbiAgICBjb25zdCBmaXJzdCA9IHBhcnNlcihzY2FubmVyKTtcbiAgICBpZiAoIWZpcnN0Lm9rKSByZXR1cm4gZmFpbHVyZSgpO1xuICAgIGNvbnN0IG91dCA9IFtcbiAgICAgIGZpcnN0LmJvZHlcbiAgICBdO1xuICAgIHdoaWxlKCFzY2FubmVyLmVvZigpKXtcbiAgICAgIGlmICghU2VwYXJhdG9yKHNjYW5uZXIpLm9rKSBicmVhaztcbiAgICAgIGNvbnN0IHJlc3VsdCA9IHBhcnNlcihzY2FubmVyKTtcbiAgICAgIGlmICghcmVzdWx0Lm9rKSB7XG4gICAgICAgIHRocm93IG5ldyBTeW50YXhFcnJvcihgSW52YWxpZCB0b2tlbiBhZnRlciBcIiR7c2VwYXJhdG9yfVwiYCk7XG4gICAgICB9XG4gICAgICBvdXQucHVzaChyZXN1bHQuYm9keSk7XG4gICAgfVxuICAgIHJldHVybiBzdWNjZXNzKG91dCk7XG4gIH07XG59XG5mdW5jdGlvbiBrdihrZXlQYXJzZXIsIHNlcGFyYXRvciwgdmFsdWVQYXJzZXIpIHtcbiAgY29uc3QgU2VwYXJhdG9yID0gY2hhcmFjdGVyKHNlcGFyYXRvcik7XG4gIHJldHVybiAoc2Nhbm5lcik9PntcbiAgICBjb25zdCBwb3NpdGlvbiA9IHNjYW5uZXIucG9zaXRpb247XG4gICAgY29uc3Qga2V5ID0ga2V5UGFyc2VyKHNjYW5uZXIpO1xuICAgIGlmICgha2V5Lm9rKSByZXR1cm4gZmFpbHVyZSgpO1xuICAgIGNvbnN0IHNlcCA9IFNlcGFyYXRvcihzY2FubmVyKTtcbiAgICBpZiAoIXNlcC5vaykge1xuICAgICAgdGhyb3cgbmV3IFN5bnRheEVycm9yKGBrZXkvdmFsdWUgcGFpciBkb2Vzbid0IGhhdmUgXCIke3NlcGFyYXRvcn1cImApO1xuICAgIH1cbiAgICBjb25zdCB2YWx1ZSA9IHZhbHVlUGFyc2VyKHNjYW5uZXIpO1xuICAgIGlmICghdmFsdWUub2spIHtcbiAgICAgIGNvbnN0IGxpbmVFbmRJbmRleCA9IHNjYW5uZXIuc291cmNlLmluZGV4T2YoXCJcXG5cIiwgc2Nhbm5lci5wb3NpdGlvbik7XG4gICAgICBjb25zdCBlbmRQb3NpdGlvbiA9IGxpbmVFbmRJbmRleCA+IDAgPyBsaW5lRW5kSW5kZXggOiBzY2FubmVyLnNvdXJjZS5sZW5ndGg7XG4gICAgICBjb25zdCBsaW5lID0gc2Nhbm5lci5zb3VyY2Uuc2xpY2UocG9zaXRpb24sIGVuZFBvc2l0aW9uKTtcbiAgICAgIHRocm93IG5ldyBTeW50YXhFcnJvcihgQ2Fubm90IHBhcnNlIHZhbHVlIG9uIGxpbmUgJyR7bGluZX0nYCk7XG4gICAgfVxuICAgIHJldHVybiBzdWNjZXNzKHVuZmxhdChrZXkuYm9keSwgdmFsdWUuYm9keSkpO1xuICB9O1xufVxuZnVuY3Rpb24gbWVyZ2UocGFyc2VyKSB7XG4gIHJldHVybiAoc2Nhbm5lcik9PntcbiAgICBjb25zdCByZXN1bHQgPSBwYXJzZXIoc2Nhbm5lcik7XG4gICAgaWYgKCFyZXN1bHQub2spIHJldHVybiBmYWlsdXJlKCk7XG4gICAgbGV0IGJvZHkgPSB7XG4gICAgICBfX3Byb3RvX186IG51bGxcbiAgICB9O1xuICAgIGZvciAoY29uc3QgcmVjb3JkIG9mIHJlc3VsdC5ib2R5KXtcbiAgICAgIGlmICh0eXBlb2YgcmVjb3JkID09PSBcIm9iamVjdFwiICYmIHJlY29yZCAhPT0gbnVsbCkge1xuICAgICAgICBib2R5ID0gZGVlcE1lcmdlKGJvZHksIHJlY29yZCk7XG4gICAgICB9XG4gICAgfVxuICAgIHJldHVybiBzdWNjZXNzKGJvZHkpO1xuICB9O1xufVxuZnVuY3Rpb24gcmVwZWF0KHBhcnNlcikge1xuICByZXR1cm4gKHNjYW5uZXIpPT57XG4gICAgY29uc3QgYm9keSA9IFtdO1xuICAgIHdoaWxlKCFzY2FubmVyLmVvZigpKXtcbiAgICAgIGNvbnN0IHJlc3VsdCA9IHBhcnNlcihzY2FubmVyKTtcbiAgICAgIGlmICghcmVzdWx0Lm9rKSBicmVhaztcbiAgICAgIGJvZHkucHVzaChyZXN1bHQuYm9keSk7XG4gICAgICBzY2FubmVyLm5leHRVbnRpbENoYXIoKTtcbiAgICB9XG4gICAgaWYgKGJvZHkubGVuZ3RoID09PSAwKSByZXR1cm4gZmFpbHVyZSgpO1xuICAgIHJldHVybiBzdWNjZXNzKGJvZHkpO1xuICB9O1xufVxuZnVuY3Rpb24gc3Vycm91bmQobGVmdCwgcGFyc2VyLCByaWdodCkge1xuICBjb25zdCBMZWZ0ID0gY2hhcmFjdGVyKGxlZnQpO1xuICBjb25zdCBSaWdodCA9IGNoYXJhY3RlcihyaWdodCk7XG4gIHJldHVybiAoc2Nhbm5lcik9PntcbiAgICBpZiAoIUxlZnQoc2Nhbm5lcikub2spIHtcbiAgICAgIHJldHVybiBmYWlsdXJlKCk7XG4gICAgfVxuICAgIGNvbnN0IHJlc3VsdCA9IHBhcnNlcihzY2FubmVyKTtcbiAgICBpZiAoIXJlc3VsdC5vaykge1xuICAgICAgdGhyb3cgbmV3IFN5bnRheEVycm9yKGBJbnZhbGlkIHRva2VuIGFmdGVyIFwiJHtsZWZ0fVwiYCk7XG4gICAgfVxuICAgIGlmICghUmlnaHQoc2Nhbm5lcikub2spIHtcbiAgICAgIHRocm93IG5ldyBTeW50YXhFcnJvcihgTm90IGNsb3NlZCBieSBcIiR7cmlnaHR9XCIgYWZ0ZXIgc3RhcnRlZCB3aXRoIFwiJHtsZWZ0fVwiYCk7XG4gICAgfVxuICAgIHJldHVybiBzdWNjZXNzKHJlc3VsdC5ib2R5KTtcbiAgfTtcbn1cbmZ1bmN0aW9uIGNoYXJhY3RlcihzdHIpIHtcbiAgcmV0dXJuIChzY2FubmVyKT0+e1xuICAgIHNjYW5uZXIuc2tpcFdoaXRlc3BhY2VzKCk7XG4gICAgaWYgKCFzY2FubmVyLnN0YXJ0c1dpdGgoc3RyKSkgcmV0dXJuIGZhaWx1cmUoKTtcbiAgICBzY2FubmVyLm5leHQoc3RyLmxlbmd0aCk7XG4gICAgc2Nhbm5lci5za2lwV2hpdGVzcGFjZXMoKTtcbiAgICByZXR1cm4gc3VjY2Vzcyh1bmRlZmluZWQpO1xuICB9O1xufVxuLy8gLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS1cbi8vIFBhcnNlciBjb21wb25lbnRzXG4vLyAtLS0tLS0tLS0tLS0tLS0tLS0tLS0tLVxuY29uc3QgQkFSRV9LRVlfUkVHRVhQID0gL1tBLVphLXowLTlfLV0rL3k7XG5leHBvcnQgZnVuY3Rpb24gYmFyZUtleShzY2FubmVyKSB7XG4gIHNjYW5uZXIuc2tpcFdoaXRlc3BhY2VzKCk7XG4gIGNvbnN0IGtleSA9IHNjYW5uZXIubWF0Y2goQkFSRV9LRVlfUkVHRVhQKT8uWzBdO1xuICBpZiAoIWtleSkgcmV0dXJuIGZhaWx1cmUoKTtcbiAgc2Nhbm5lci5uZXh0KGtleS5sZW5ndGgpO1xuICByZXR1cm4gc3VjY2VzcyhrZXkpO1xufVxuZnVuY3Rpb24gZXNjYXBlU2VxdWVuY2Uoc2Nhbm5lcikge1xuICBpZiAoc2Nhbm5lci5jaGFyKCkgIT09IFwiXFxcXFwiKSByZXR1cm4gZmFpbHVyZSgpO1xuICBzY2FubmVyLm5leHQoKTtcbiAgLy8gU2VlIGh0dHBzOi8vdG9tbC5pby9lbi92MS4wLjAtcmMuMyNzdHJpbmdcbiAgc3dpdGNoKHNjYW5uZXIuY2hhcigpKXtcbiAgICBjYXNlIFwiYlwiOlxuICAgICAgc2Nhbm5lci5uZXh0KCk7XG4gICAgICByZXR1cm4gc3VjY2VzcyhcIlxcYlwiKTtcbiAgICBjYXNlIFwidFwiOlxuICAgICAgc2Nhbm5lci5uZXh0KCk7XG4gICAgICByZXR1cm4gc3VjY2VzcyhcIlxcdFwiKTtcbiAgICBjYXNlIFwiblwiOlxuICAgICAgc2Nhbm5lci5uZXh0KCk7XG4gICAgICByZXR1cm4gc3VjY2VzcyhcIlxcblwiKTtcbiAgICBjYXNlIFwiZlwiOlxuICAgICAgc2Nhbm5lci5uZXh0KCk7XG4gICAgICByZXR1cm4gc3VjY2VzcyhcIlxcZlwiKTtcbiAgICBjYXNlIFwiclwiOlxuICAgICAgc2Nhbm5lci5uZXh0KCk7XG4gICAgICByZXR1cm4gc3VjY2VzcyhcIlxcclwiKTtcbiAgICBjYXNlIFwidVwiOlxuICAgIGNhc2UgXCJVXCI6XG4gICAgICB7XG4gICAgICAgIC8vIFVuaWNvZGUgY2hhcmFjdGVyXG4gICAgICAgIGNvbnN0IGNvZGVQb2ludExlbiA9IHNjYW5uZXIuY2hhcigpID09PSBcInVcIiA/IDQgOiA2O1xuICAgICAgICBjb25zdCBjb2RlUG9pbnQgPSBwYXJzZUludChcIjB4XCIgKyBzY2FubmVyLnNsaWNlKDEsIDEgKyBjb2RlUG9pbnRMZW4pLCAxNik7XG4gICAgICAgIGNvbnN0IHN0ciA9IFN0cmluZy5mcm9tQ29kZVBvaW50KGNvZGVQb2ludCk7XG4gICAgICAgIHNjYW5uZXIubmV4dChjb2RlUG9pbnRMZW4gKyAxKTtcbiAgICAgICAgcmV0dXJuIHN1Y2Nlc3Moc3RyKTtcbiAgICAgIH1cbiAgICBjYXNlICdcIic6XG4gICAgICBzY2FubmVyLm5leHQoKTtcbiAgICAgIHJldHVybiBzdWNjZXNzKCdcIicpO1xuICAgIGNhc2UgXCJcXFxcXCI6XG4gICAgICBzY2FubmVyLm5leHQoKTtcbiAgICAgIHJldHVybiBzdWNjZXNzKFwiXFxcXFwiKTtcbiAgICBkZWZhdWx0OlxuICAgICAgdGhyb3cgbmV3IFN5bnRheEVycm9yKGBJbnZhbGlkIGVzY2FwZSBzZXF1ZW5jZTogXFxcXCR7c2Nhbm5lci5jaGFyKCl9YCk7XG4gIH1cbn1cbmV4cG9ydCBmdW5jdGlvbiBiYXNpY1N0cmluZyhzY2FubmVyKSB7XG4gIHNjYW5uZXIuc2tpcFdoaXRlc3BhY2VzKCk7XG4gIGlmIChzY2FubmVyLmNoYXIoKSAhPT0gJ1wiJykgcmV0dXJuIGZhaWx1cmUoKTtcbiAgc2Nhbm5lci5uZXh0KCk7XG4gIGNvbnN0IGFjYyA9IFtdO1xuICB3aGlsZShzY2FubmVyLmNoYXIoKSAhPT0gJ1wiJyAmJiAhc2Nhbm5lci5lb2YoKSl7XG4gICAgaWYgKHNjYW5uZXIuY2hhcigpID09PSBcIlxcblwiKSB7XG4gICAgICB0aHJvdyBuZXcgU3ludGF4RXJyb3IoXCJTaW5nbGUtbGluZSBzdHJpbmcgY2Fubm90IGNvbnRhaW4gRU9MXCIpO1xuICAgIH1cbiAgICBjb25zdCBlc2NhcGVkQ2hhciA9IGVzY2FwZVNlcXVlbmNlKHNjYW5uZXIpO1xuICAgIGlmIChlc2NhcGVkQ2hhci5vaykge1xuICAgICAgYWNjLnB1c2goZXNjYXBlZENoYXIuYm9keSk7XG4gICAgfSBlbHNlIHtcbiAgICAgIGFjYy5wdXNoKHNjYW5uZXIuY2hhcigpKTtcbiAgICAgIHNjYW5uZXIubmV4dCgpO1xuICAgIH1cbiAgfVxuICBpZiAoc2Nhbm5lci5lb2YoKSkge1xuICAgIHRocm93IG5ldyBTeW50YXhFcnJvcihgU2luZ2xlLWxpbmUgc3RyaW5nIGlzIG5vdCBjbG9zZWQ6XFxuJHthY2Muam9pbihcIlwiKX1gKTtcbiAgfVxuICBzY2FubmVyLm5leHQoKTsgLy8gc2tpcCBsYXN0ICdcIlwiXG4gIHJldHVybiBzdWNjZXNzKGFjYy5qb2luKFwiXCIpKTtcbn1cbmV4cG9ydCBmdW5jdGlvbiBsaXRlcmFsU3RyaW5nKHNjYW5uZXIpIHtcbiAgc2Nhbm5lci5za2lwV2hpdGVzcGFjZXMoKTtcbiAgaWYgKHNjYW5uZXIuY2hhcigpICE9PSBcIidcIikgcmV0dXJuIGZhaWx1cmUoKTtcbiAgc2Nhbm5lci5uZXh0KCk7XG4gIGNvbnN0IGFjYyA9IFtdO1xuICB3aGlsZShzY2FubmVyLmNoYXIoKSAhPT0gXCInXCIgJiYgIXNjYW5uZXIuZW9mKCkpe1xuICAgIGlmIChzY2FubmVyLmNoYXIoKSA9PT0gXCJcXG5cIikge1xuICAgICAgdGhyb3cgbmV3IFN5bnRheEVycm9yKFwiU2luZ2xlLWxpbmUgc3RyaW5nIGNhbm5vdCBjb250YWluIEVPTFwiKTtcbiAgICB9XG4gICAgYWNjLnB1c2goc2Nhbm5lci5jaGFyKCkpO1xuICAgIHNjYW5uZXIubmV4dCgpO1xuICB9XG4gIGlmIChzY2FubmVyLmVvZigpKSB7XG4gICAgdGhyb3cgbmV3IFN5bnRheEVycm9yKGBTaW5nbGUtbGluZSBzdHJpbmcgaXMgbm90IGNsb3NlZDpcXG4ke2FjYy5qb2luKFwiXCIpfWApO1xuICB9XG4gIHNjYW5uZXIubmV4dCgpOyAvLyBza2lwIGxhc3QgXCInXCJcbiAgcmV0dXJuIHN1Y2Nlc3MoYWNjLmpvaW4oXCJcIikpO1xufVxuZXhwb3J0IGZ1bmN0aW9uIG11bHRpbGluZUJhc2ljU3RyaW5nKHNjYW5uZXIpIHtcbiAgc2Nhbm5lci5za2lwV2hpdGVzcGFjZXMoKTtcbiAgaWYgKCFzY2FubmVyLnN0YXJ0c1dpdGgoJ1wiXCJcIicpKSByZXR1cm4gZmFpbHVyZSgpO1xuICBzY2FubmVyLm5leHQoMyk7XG4gIGlmIChzY2FubmVyLmNoYXIoKSA9PT0gXCJcXG5cIikge1xuICAgIC8vIFRoZSBmaXJzdCBuZXdsaW5lIChMRikgaXMgdHJpbW1lZFxuICAgIHNjYW5uZXIubmV4dCgpO1xuICB9IGVsc2UgaWYgKHNjYW5uZXIuc3RhcnRzV2l0aChcIlxcclxcblwiKSkge1xuICAgIC8vIFRoZSBmaXJzdCBuZXdsaW5lIChDUkxGKSBpcyB0cmltbWVkXG4gICAgc2Nhbm5lci5uZXh0KDIpO1xuICB9XG4gIGNvbnN0IGFjYyA9IFtdO1xuICB3aGlsZSghc2Nhbm5lci5zdGFydHNXaXRoKCdcIlwiXCInKSAmJiAhc2Nhbm5lci5lb2YoKSl7XG4gICAgLy8gbGluZSBlbmRpbmcgYmFja3NsYXNoXG4gICAgaWYgKHNjYW5uZXIuc3RhcnRzV2l0aChcIlxcXFxcXG5cIikpIHtcbiAgICAgIHNjYW5uZXIubmV4dCgpO1xuICAgICAgc2Nhbm5lci5uZXh0VW50aWxDaGFyKHtcbiAgICAgICAgc2tpcENvbW1lbnRzOiBmYWxzZVxuICAgICAgfSk7XG4gICAgICBjb250aW51ZTtcbiAgICB9IGVsc2UgaWYgKHNjYW5uZXIuc3RhcnRzV2l0aChcIlxcXFxcXHJcXG5cIikpIHtcbiAgICAgIHNjYW5uZXIubmV4dCgpO1xuICAgICAgc2Nhbm5lci5uZXh0VW50aWxDaGFyKHtcbiAgICAgICAgc2tpcENvbW1lbnRzOiBmYWxzZVxuICAgICAgfSk7XG4gICAgICBjb250aW51ZTtcbiAgICB9XG4gICAgY29uc3QgZXNjYXBlZENoYXIgPSBlc2NhcGVTZXF1ZW5jZShzY2FubmVyKTtcbiAgICBpZiAoZXNjYXBlZENoYXIub2spIHtcbiAgICAgIGFjYy5wdXNoKGVzY2FwZWRDaGFyLmJvZHkpO1xuICAgIH0gZWxzZSB7XG4gICAgICBhY2MucHVzaChzY2FubmVyLmNoYXIoKSk7XG4gICAgICBzY2FubmVyLm5leHQoKTtcbiAgICB9XG4gIH1cbiAgaWYgKHNjYW5uZXIuZW9mKCkpIHtcbiAgICB0aHJvdyBuZXcgU3ludGF4RXJyb3IoYE11bHRpLWxpbmUgc3RyaW5nIGlzIG5vdCBjbG9zZWQ6XFxuJHthY2Muam9pbihcIlwiKX1gKTtcbiAgfVxuICAvLyBpZiBlbmRzIHdpdGggNCBgXCJgLCBwdXNoIHRoZSBmaXN0IGBcImAgdG8gc3RyaW5nXG4gIGlmIChzY2FubmVyLmNoYXIoMykgPT09ICdcIicpIHtcbiAgICBhY2MucHVzaCgnXCInKTtcbiAgICBzY2FubmVyLm5leHQoKTtcbiAgfVxuICBzY2FubmVyLm5leHQoMyk7IC8vIHNraXAgbGFzdCAnXCJcIlwiXCJcbiAgcmV0dXJuIHN1Y2Nlc3MoYWNjLmpvaW4oXCJcIikpO1xufVxuZXhwb3J0IGZ1bmN0aW9uIG11bHRpbGluZUxpdGVyYWxTdHJpbmcoc2Nhbm5lcikge1xuICBzY2FubmVyLnNraXBXaGl0ZXNwYWNlcygpO1xuICBpZiAoIXNjYW5uZXIuc3RhcnRzV2l0aChcIicnJ1wiKSkgcmV0dXJuIGZhaWx1cmUoKTtcbiAgc2Nhbm5lci5uZXh0KDMpO1xuICBpZiAoc2Nhbm5lci5jaGFyKCkgPT09IFwiXFxuXCIpIHtcbiAgICAvLyBUaGUgZmlyc3QgbmV3bGluZSAoTEYpIGlzIHRyaW1tZWRcbiAgICBzY2FubmVyLm5leHQoKTtcbiAgfSBlbHNlIGlmIChzY2FubmVyLnN0YXJ0c1dpdGgoXCJcXHJcXG5cIikpIHtcbiAgICAvLyBUaGUgZmlyc3QgbmV3bGluZSAoQ1JMRikgaXMgdHJpbW1lZFxuICAgIHNjYW5uZXIubmV4dCgyKTtcbiAgfVxuICBjb25zdCBhY2MgPSBbXTtcbiAgd2hpbGUoIXNjYW5uZXIuc3RhcnRzV2l0aChcIicnJ1wiKSAmJiAhc2Nhbm5lci5lb2YoKSl7XG4gICAgYWNjLnB1c2goc2Nhbm5lci5jaGFyKCkpO1xuICAgIHNjYW5uZXIubmV4dCgpO1xuICB9XG4gIGlmIChzY2FubmVyLmVvZigpKSB7XG4gICAgdGhyb3cgbmV3IFN5bnRheEVycm9yKGBNdWx0aS1saW5lIHN0cmluZyBpcyBub3QgY2xvc2VkOlxcbiR7YWNjLmpvaW4oXCJcIil9YCk7XG4gIH1cbiAgLy8gaWYgZW5kcyB3aXRoIDQgYCdgLCBwdXNoIHRoZSBmaXN0IGAnYCB0byBzdHJpbmdcbiAgaWYgKHNjYW5uZXIuY2hhcigzKSA9PT0gXCInXCIpIHtcbiAgICBhY2MucHVzaChcIidcIik7XG4gICAgc2Nhbm5lci5uZXh0KCk7XG4gIH1cbiAgc2Nhbm5lci5uZXh0KDMpOyAvLyBza2lwIGxhc3QgXCInJydcIlxuICByZXR1cm4gc3VjY2VzcyhhY2Muam9pbihcIlwiKSk7XG59XG5jb25zdCBCT09MRUFOX1JFR0VYUCA9IC8oPzp0cnVlfGZhbHNlKVxcYi95O1xuZXhwb3J0IGZ1bmN0aW9uIGJvb2xlYW4oc2Nhbm5lcikge1xuICBzY2FubmVyLnNraXBXaGl0ZXNwYWNlcygpO1xuICBjb25zdCBtYXRjaCA9IHNjYW5uZXIubWF0Y2goQk9PTEVBTl9SRUdFWFApO1xuICBpZiAoIW1hdGNoKSByZXR1cm4gZmFpbHVyZSgpO1xuICBjb25zdCBzdHJpbmcgPSBtYXRjaFswXTtcbiAgc2Nhbm5lci5uZXh0KHN0cmluZy5sZW5ndGgpO1xuICBjb25zdCB2YWx1ZSA9IHN0cmluZyA9PT0gXCJ0cnVlXCI7XG4gIHJldHVybiBzdWNjZXNzKHZhbHVlKTtcbn1cbmNvbnN0IElORklOSVRZX01BUCA9IG5ldyBNYXAoW1xuICBbXG4gICAgXCJpbmZcIixcbiAgICBJbmZpbml0eVxuICBdLFxuICBbXG4gICAgXCIraW5mXCIsXG4gICAgSW5maW5pdHlcbiAgXSxcbiAgW1xuICAgIFwiLWluZlwiLFxuICAgIC1JbmZpbml0eVxuICBdXG5dKTtcbmNvbnN0IElORklOSVRZX1JFR0VYUCA9IC9bKy1dP2luZlxcYi95O1xuZXhwb3J0IGZ1bmN0aW9uIGluZmluaXR5KHNjYW5uZXIpIHtcbiAgc2Nhbm5lci5za2lwV2hpdGVzcGFjZXMoKTtcbiAgY29uc3QgbWF0Y2ggPSBzY2FubmVyLm1hdGNoKElORklOSVRZX1JFR0VYUCk7XG4gIGlmICghbWF0Y2gpIHJldHVybiBmYWlsdXJlKCk7XG4gIGNvbnN0IHN0cmluZyA9IG1hdGNoWzBdO1xuICBzY2FubmVyLm5leHQoc3RyaW5nLmxlbmd0aCk7XG4gIGNvbnN0IHZhbHVlID0gSU5GSU5JVFlfTUFQLmdldChzdHJpbmcpO1xuICByZXR1cm4gc3VjY2Vzcyh2YWx1ZSk7XG59XG5jb25zdCBOQU5fUkVHRVhQID0gL1srLV0/bmFuXFxiL3k7XG5leHBvcnQgZnVuY3Rpb24gbmFuKHNjYW5uZXIpIHtcbiAgc2Nhbm5lci5za2lwV2hpdGVzcGFjZXMoKTtcbiAgY29uc3QgbWF0Y2ggPSBzY2FubmVyLm1hdGNoKE5BTl9SRUdFWFApO1xuICBpZiAoIW1hdGNoKSByZXR1cm4gZmFpbHVyZSgpO1xuICBjb25zdCBzdHJpbmcgPSBtYXRjaFswXTtcbiAgc2Nhbm5lci5uZXh0KHN0cmluZy5sZW5ndGgpO1xuICBjb25zdCB2YWx1ZSA9IE5hTjtcbiAgcmV0dXJuIHN1Y2Nlc3ModmFsdWUpO1xufVxuZXhwb3J0IGNvbnN0IGRvdHRlZEtleSA9IGpvaW4xKG9yKFtcbiAgYmFyZUtleSxcbiAgYmFzaWNTdHJpbmcsXG4gIGxpdGVyYWxTdHJpbmdcbl0pLCBcIi5cIik7XG5jb25zdCBCSU5BUllfUkVHRVhQID0gLzBiWzAxXSsoPzpfWzAxXSspKlxcYi95O1xuZXhwb3J0IGZ1bmN0aW9uIGJpbmFyeShzY2FubmVyKSB7XG4gIHNjYW5uZXIuc2tpcFdoaXRlc3BhY2VzKCk7XG4gIGNvbnN0IG1hdGNoID0gc2Nhbm5lci5tYXRjaChCSU5BUllfUkVHRVhQKT8uWzBdO1xuICBpZiAoIW1hdGNoKSByZXR1cm4gZmFpbHVyZSgpO1xuICBzY2FubmVyLm5leHQobWF0Y2gubGVuZ3RoKTtcbiAgY29uc3QgdmFsdWUgPSBtYXRjaC5zbGljZSgyKS5yZXBsYWNlQWxsKFwiX1wiLCBcIlwiKTtcbiAgY29uc3QgbnVtYmVyID0gcGFyc2VJbnQodmFsdWUsIDIpO1xuICByZXR1cm4gaXNOYU4obnVtYmVyKSA/IGZhaWx1cmUoKSA6IHN1Y2Nlc3MobnVtYmVyKTtcbn1cbmNvbnN0IE9DVEFMX1JFR0VYUCA9IC8wb1swLTddKyg/Ol9bMC03XSspKlxcYi95O1xuZXhwb3J0IGZ1bmN0aW9uIG9jdGFsKHNjYW5uZXIpIHtcbiAgc2Nhbm5lci5za2lwV2hpdGVzcGFjZXMoKTtcbiAgY29uc3QgbWF0Y2ggPSBzY2FubmVyLm1hdGNoKE9DVEFMX1JFR0VYUCk/LlswXTtcbiAgaWYgKCFtYXRjaCkgcmV0dXJuIGZhaWx1cmUoKTtcbiAgc2Nhbm5lci5uZXh0KG1hdGNoLmxlbmd0aCk7XG4gIGNvbnN0IHZhbHVlID0gbWF0Y2guc2xpY2UoMikucmVwbGFjZUFsbChcIl9cIiwgXCJcIik7XG4gIGNvbnN0IG51bWJlciA9IHBhcnNlSW50KHZhbHVlLCA4KTtcbiAgcmV0dXJuIGlzTmFOKG51bWJlcikgPyBmYWlsdXJlKCkgOiBzdWNjZXNzKG51bWJlcik7XG59XG5jb25zdCBIRVhfUkVHRVhQID0gLzB4WzAtOWEtZl0rKD86X1swLTlhLWZdKykqXFxiL3lpO1xuZXhwb3J0IGZ1bmN0aW9uIGhleChzY2FubmVyKSB7XG4gIHNjYW5uZXIuc2tpcFdoaXRlc3BhY2VzKCk7XG4gIGNvbnN0IG1hdGNoID0gc2Nhbm5lci5tYXRjaChIRVhfUkVHRVhQKT8uWzBdO1xuICBpZiAoIW1hdGNoKSByZXR1cm4gZmFpbHVyZSgpO1xuICBzY2FubmVyLm5leHQobWF0Y2gubGVuZ3RoKTtcbiAgY29uc3QgdmFsdWUgPSBtYXRjaC5zbGljZSgyKS5yZXBsYWNlQWxsKFwiX1wiLCBcIlwiKTtcbiAgY29uc3QgbnVtYmVyID0gcGFyc2VJbnQodmFsdWUsIDE2KTtcbiAgcmV0dXJuIGlzTmFOKG51bWJlcikgPyBmYWlsdXJlKCkgOiBzdWNjZXNzKG51bWJlcik7XG59XG5jb25zdCBJTlRFR0VSX1JFR0VYUCA9IC9bKy1dPyg/OjB8WzEtOV1bMC05XSooPzpfWzAtOV0rKSopXFxiL3k7XG5leHBvcnQgZnVuY3Rpb24gaW50ZWdlcihzY2FubmVyKSB7XG4gIHNjYW5uZXIuc2tpcFdoaXRlc3BhY2VzKCk7XG4gIGNvbnN0IG1hdGNoID0gc2Nhbm5lci5tYXRjaChJTlRFR0VSX1JFR0VYUCk/LlswXTtcbiAgaWYgKCFtYXRjaCkgcmV0dXJuIGZhaWx1cmUoKTtcbiAgc2Nhbm5lci5uZXh0KG1hdGNoLmxlbmd0aCk7XG4gIGNvbnN0IHZhbHVlID0gbWF0Y2gucmVwbGFjZUFsbChcIl9cIiwgXCJcIik7XG4gIGNvbnN0IGludCA9IHBhcnNlSW50KHZhbHVlLCAxMCk7XG4gIHJldHVybiBzdWNjZXNzKGludCk7XG59XG5jb25zdCBGTE9BVF9SRUdFWFAgPSAvWystXT8oPzowfFsxLTldWzAtOV0qKD86X1swLTldKykqKSg/OlxcLlswLTldKyg/Ol9bMC05XSspKik/KD86ZVsrLV0/WzAtOV0rKD86X1swLTldKykqKT9cXGIveWk7XG5leHBvcnQgZnVuY3Rpb24gZmxvYXQoc2Nhbm5lcikge1xuICBzY2FubmVyLnNraXBXaGl0ZXNwYWNlcygpO1xuICBjb25zdCBtYXRjaCA9IHNjYW5uZXIubWF0Y2goRkxPQVRfUkVHRVhQKT8uWzBdO1xuICBpZiAoIW1hdGNoKSByZXR1cm4gZmFpbHVyZSgpO1xuICBzY2FubmVyLm5leHQobWF0Y2gubGVuZ3RoKTtcbiAgY29uc3QgdmFsdWUgPSBtYXRjaC5yZXBsYWNlQWxsKFwiX1wiLCBcIlwiKTtcbiAgY29uc3QgZmxvYXQgPSBwYXJzZUZsb2F0KHZhbHVlKTtcbiAgaWYgKGlzTmFOKGZsb2F0KSkgcmV0dXJuIGZhaWx1cmUoKTtcbiAgcmV0dXJuIHN1Y2Nlc3MoZmxvYXQpO1xufVxuY29uc3QgREFURV9USU1FX1JFR0VYUCA9IC8oPzx5ZWFyPlxcZHs0fSktKD88bW9udGg+XFxkezJ9KS0oPzxkYXk+XFxkezJ9KSg/OlsgMC05VFouOistXSspP1xcYi95O1xuZXhwb3J0IGZ1bmN0aW9uIGRhdGVUaW1lKHNjYW5uZXIpIHtcbiAgc2Nhbm5lci5za2lwV2hpdGVzcGFjZXMoKTtcbiAgY29uc3QgbWF0Y2ggPSBzY2FubmVyLm1hdGNoKERBVEVfVElNRV9SRUdFWFApO1xuICBpZiAoIW1hdGNoKSByZXR1cm4gZmFpbHVyZSgpO1xuICBjb25zdCBzdHJpbmcgPSBtYXRjaFswXTtcbiAgc2Nhbm5lci5uZXh0KHN0cmluZy5sZW5ndGgpO1xuICBjb25zdCBncm91cHMgPSBtYXRjaC5ncm91cHM7XG4gIC8vIHNwZWNpYWwgY2FzZSBpZiBtb250aCBpcyBGZWJydWFyeVxuICBpZiAoZ3JvdXBzLm1vbnRoID09IFwiMDJcIikge1xuICAgIGNvbnN0IGRheXMgPSBwYXJzZUludChncm91cHMuZGF5KTtcbiAgICBpZiAoZGF5cyA+IDI5KSB7XG4gICAgICB0aHJvdyBuZXcgU3ludGF4RXJyb3IoYEludmFsaWQgZGF0ZSBzdHJpbmcgXCIke21hdGNofVwiYCk7XG4gICAgfVxuICAgIGNvbnN0IHllYXIgPSBwYXJzZUludChncm91cHMueWVhcik7XG4gICAgaWYgKGRheXMgPiAyOCAmJiAhaXNMZWFwKHllYXIpKSB7XG4gICAgICB0aHJvdyBuZXcgU3ludGF4RXJyb3IoYEludmFsaWQgZGF0ZSBzdHJpbmcgXCIke21hdGNofVwiYCk7XG4gICAgfVxuICB9XG4gIGNvbnN0IGRhdGUgPSBuZXcgRGF0ZShzdHJpbmcudHJpbSgpKTtcbiAgLy8gaW52YWxpZCBkYXRlXG4gIGlmIChpc05hTihkYXRlLmdldFRpbWUoKSkpIHtcbiAgICB0aHJvdyBuZXcgU3ludGF4RXJyb3IoYEludmFsaWQgZGF0ZSBzdHJpbmcgXCIke21hdGNofVwiYCk7XG4gIH1cbiAgcmV0dXJuIHN1Y2Nlc3MoZGF0ZSk7XG59XG5jb25zdCBMT0NBTF9USU1FX1JFR0VYUCA9IC8oXFxkezJ9KTooXFxkezJ9KTooXFxkezJ9KSg/OlxcLlswLTldKyk/XFxiL3k7XG5leHBvcnQgZnVuY3Rpb24gbG9jYWxUaW1lKHNjYW5uZXIpIHtcbiAgc2Nhbm5lci5za2lwV2hpdGVzcGFjZXMoKTtcbiAgY29uc3QgbWF0Y2ggPSBzY2FubmVyLm1hdGNoKExPQ0FMX1RJTUVfUkVHRVhQKT8uWzBdO1xuICBpZiAoIW1hdGNoKSByZXR1cm4gZmFpbHVyZSgpO1xuICBzY2FubmVyLm5leHQobWF0Y2gubGVuZ3RoKTtcbiAgcmV0dXJuIHN1Y2Nlc3MobWF0Y2gpO1xufVxuZXhwb3J0IGZ1bmN0aW9uIGFycmF5VmFsdWUoc2Nhbm5lcikge1xuICBzY2FubmVyLnNraXBXaGl0ZXNwYWNlcygpO1xuICBpZiAoc2Nhbm5lci5jaGFyKCkgIT09IFwiW1wiKSByZXR1cm4gZmFpbHVyZSgpO1xuICBzY2FubmVyLm5leHQoKTtcbiAgY29uc3QgYXJyYXkgPSBbXTtcbiAgd2hpbGUoIXNjYW5uZXIuZW9mKCkpe1xuICAgIHNjYW5uZXIubmV4dFVudGlsQ2hhcigpO1xuICAgIGNvbnN0IHJlc3VsdCA9IHZhbHVlKHNjYW5uZXIpO1xuICAgIGlmICghcmVzdWx0Lm9rKSBicmVhaztcbiAgICBhcnJheS5wdXNoKHJlc3VsdC5ib2R5KTtcbiAgICBzY2FubmVyLnNraXBXaGl0ZXNwYWNlcygpO1xuICAgIC8vIG1heSBoYXZlIGEgbmV4dCBpdGVtLCBidXQgdHJhaWxpbmcgY29tbWEgaXMgYWxsb3dlZCBhdCBhcnJheVxuICAgIGlmIChzY2FubmVyLmNoYXIoKSAhPT0gXCIsXCIpIGJyZWFrO1xuICAgIHNjYW5uZXIubmV4dCgpO1xuICB9XG4gIHNjYW5uZXIubmV4dFVudGlsQ2hhcigpO1xuICBpZiAoc2Nhbm5lci5jaGFyKCkgIT09IFwiXVwiKSB0aHJvdyBuZXcgU3ludGF4RXJyb3IoXCJBcnJheSBpcyBub3QgY2xvc2VkXCIpO1xuICBzY2FubmVyLm5leHQoKTtcbiAgcmV0dXJuIHN1Y2Nlc3MoYXJyYXkpO1xufVxuZXhwb3J0IGZ1bmN0aW9uIGlubGluZVRhYmxlKHNjYW5uZXIpIHtcbiAgc2Nhbm5lci5uZXh0VW50aWxDaGFyKCk7XG4gIGlmIChzY2FubmVyLmNoYXIoMSkgPT09IFwifVwiKSB7XG4gICAgc2Nhbm5lci5uZXh0KDIpO1xuICAgIHJldHVybiBzdWNjZXNzKHtcbiAgICAgIF9fcHJvdG9fXzogbnVsbFxuICAgIH0pO1xuICB9XG4gIGNvbnN0IHBhaXJzID0gc3Vycm91bmQoXCJ7XCIsIGpvaW4ocGFpciwgXCIsXCIpLCBcIn1cIikoc2Nhbm5lcik7XG4gIGlmICghcGFpcnMub2spIHJldHVybiBmYWlsdXJlKCk7XG4gIGxldCB0YWJsZSA9IHtcbiAgICBfX3Byb3RvX186IG51bGxcbiAgfTtcbiAgZm9yIChjb25zdCBwYWlyIG9mIHBhaXJzLmJvZHkpe1xuICAgIHRhYmxlID0gZGVlcE1lcmdlKHRhYmxlLCBwYWlyKTtcbiAgfVxuICByZXR1cm4gc3VjY2Vzcyh0YWJsZSk7XG59XG5leHBvcnQgY29uc3QgdmFsdWUgPSBvcihbXG4gIG11bHRpbGluZUJhc2ljU3RyaW5nLFxuICBtdWx0aWxpbmVMaXRlcmFsU3RyaW5nLFxuICBiYXNpY1N0cmluZyxcbiAgbGl0ZXJhbFN0cmluZyxcbiAgYm9vbGVhbixcbiAgaW5maW5pdHksXG4gIG5hbixcbiAgZGF0ZVRpbWUsXG4gIGxvY2FsVGltZSxcbiAgYmluYXJ5LFxuICBvY3RhbCxcbiAgaGV4LFxuICBmbG9hdCxcbiAgaW50ZWdlcixcbiAgYXJyYXlWYWx1ZSxcbiAgaW5saW5lVGFibGVcbl0pO1xuZXhwb3J0IGNvbnN0IHBhaXIgPSBrdihkb3R0ZWRLZXksIFwiPVwiLCB2YWx1ZSk7XG5leHBvcnQgZnVuY3Rpb24gYmxvY2soc2Nhbm5lcikge1xuICBzY2FubmVyLm5leHRVbnRpbENoYXIoKTtcbiAgY29uc3QgcmVzdWx0ID0gbWVyZ2UocmVwZWF0KHBhaXIpKShzY2FubmVyKTtcbiAgaWYgKHJlc3VsdC5vaykgcmV0dXJuIHN1Y2Nlc3Moe1xuICAgIHR5cGU6IFwiQmxvY2tcIixcbiAgICB2YWx1ZTogcmVzdWx0LmJvZHlcbiAgfSk7XG4gIHJldHVybiBmYWlsdXJlKCk7XG59XG5leHBvcnQgY29uc3QgdGFibGVIZWFkZXIgPSBzdXJyb3VuZChcIltcIiwgZG90dGVkS2V5LCBcIl1cIik7XG5leHBvcnQgZnVuY3Rpb24gdGFibGUoc2Nhbm5lcikge1xuICBzY2FubmVyLm5leHRVbnRpbENoYXIoKTtcbiAgY29uc3QgaGVhZGVyID0gdGFibGVIZWFkZXIoc2Nhbm5lcik7XG4gIGlmICghaGVhZGVyLm9rKSByZXR1cm4gZmFpbHVyZSgpO1xuICBzY2FubmVyLm5leHRVbnRpbENoYXIoKTtcbiAgY29uc3QgYiA9IGJsb2NrKHNjYW5uZXIpO1xuICByZXR1cm4gc3VjY2Vzcyh7XG4gICAgdHlwZTogXCJUYWJsZVwiLFxuICAgIGtleXM6IGhlYWRlci5ib2R5LFxuICAgIHZhbHVlOiBiLm9rID8gYi5ib2R5LnZhbHVlIDoge1xuICAgICAgX19wcm90b19fOiBudWxsXG4gICAgfVxuICB9KTtcbn1cbmV4cG9ydCBjb25zdCB0YWJsZUFycmF5SGVhZGVyID0gc3Vycm91bmQoXCJbW1wiLCBkb3R0ZWRLZXksIFwiXV1cIik7XG5leHBvcnQgZnVuY3Rpb24gdGFibGVBcnJheShzY2FubmVyKSB7XG4gIHNjYW5uZXIubmV4dFVudGlsQ2hhcigpO1xuICBjb25zdCBoZWFkZXIgPSB0YWJsZUFycmF5SGVhZGVyKHNjYW5uZXIpO1xuICBpZiAoIWhlYWRlci5vaykgcmV0dXJuIGZhaWx1cmUoKTtcbiAgc2Nhbm5lci5uZXh0VW50aWxDaGFyKCk7XG4gIGNvbnN0IGIgPSBibG9jayhzY2FubmVyKTtcbiAgcmV0dXJuIHN1Y2Nlc3Moe1xuICAgIHR5cGU6IFwiVGFibGVBcnJheVwiLFxuICAgIGtleXM6IGhlYWRlci5ib2R5LFxuICAgIHZhbHVlOiBiLm9rID8gYi5ib2R5LnZhbHVlIDoge1xuICAgICAgX19wcm90b19fOiBudWxsXG4gICAgfVxuICB9KTtcbn1cbmV4cG9ydCBmdW5jdGlvbiB0b21sKHNjYW5uZXIpIHtcbiAgY29uc3QgYmxvY2tzID0gcmVwZWF0KG9yKFtcbiAgICBibG9jayxcbiAgICB0YWJsZUFycmF5LFxuICAgIHRhYmxlXG4gIF0pKShzY2FubmVyKTtcbiAgaWYgKCFibG9ja3Mub2spIHJldHVybiBzdWNjZXNzKHtcbiAgICBfX3Byb3RvX186IG51bGxcbiAgfSk7XG4gIGNvbnN0IGJvZHkgPSBibG9ja3MuYm9keS5yZWR1Y2UoZGVlcEFzc2lnbiwge1xuICAgIF9fcHJvdG9fXzogbnVsbFxuICB9KTtcbiAgcmV0dXJuIHN1Y2Nlc3MoYm9keSk7XG59XG5mdW5jdGlvbiBjcmVhdGVQYXJzZUVycm9yTWVzc2FnZShzY2FubmVyLCBtZXNzYWdlKSB7XG4gIGNvbnN0IHN0cmluZyA9IHNjYW5uZXIuc291cmNlLnNsaWNlKDAsIHNjYW5uZXIucG9zaXRpb24pO1xuICBjb25zdCBsaW5lcyA9IHN0cmluZy5zcGxpdChcIlxcblwiKTtcbiAgY29uc3Qgcm93ID0gbGluZXMubGVuZ3RoO1xuICBjb25zdCBjb2x1bW4gPSBsaW5lcy5hdCgtMSk/Lmxlbmd0aCA/PyAwO1xuICByZXR1cm4gYFBhcnNlIGVycm9yIG9uIGxpbmUgJHtyb3d9LCBjb2x1bW4gJHtjb2x1bW59OiAke21lc3NhZ2V9YDtcbn1cbmV4cG9ydCBmdW5jdGlvbiBwYXJzZXJGYWN0b3J5KHBhcnNlcikge1xuICByZXR1cm4gKHRvbWxTdHJpbmcpPT57XG4gICAgY29uc3Qgc2Nhbm5lciA9IG5ldyBTY2FubmVyKHRvbWxTdHJpbmcpO1xuICAgIHRyeSB7XG4gICAgICBjb25zdCByZXN1bHQgPSBwYXJzZXIoc2Nhbm5lcik7XG4gICAgICBpZiAocmVzdWx0Lm9rICYmIHNjYW5uZXIuZW9mKCkpIHJldHVybiByZXN1bHQuYm9keTtcbiAgICAgIGNvbnN0IG1lc3NhZ2UgPSBgVW5leHBlY3RlZCBjaGFyYWN0ZXI6IFwiJHtzY2FubmVyLmNoYXIoKX1cImA7XG4gICAgICB0aHJvdyBuZXcgU3ludGF4RXJyb3IoY3JlYXRlUGFyc2VFcnJvck1lc3NhZ2Uoc2Nhbm5lciwgbWVzc2FnZSkpO1xuICAgIH0gY2F0Y2ggKGVycm9yKSB7XG4gICAgICBpZiAoZXJyb3IgaW5zdGFuY2VvZiBFcnJvcikge1xuICAgICAgICB0aHJvdyBuZXcgU3ludGF4RXJyb3IoY3JlYXRlUGFyc2VFcnJvck1lc3NhZ2Uoc2Nhbm5lciwgZXJyb3IubWVzc2FnZSkpO1xuICAgICAgfVxuICAgICAgY29uc3QgbWVzc2FnZSA9IFwiSW52YWxpZCBlcnJvciB0eXBlIGNhdWdodFwiO1xuICAgICAgdGhyb3cgbmV3IFN5bnRheEVycm9yKGNyZWF0ZVBhcnNlRXJyb3JNZXNzYWdlKHNjYW5uZXIsIG1lc3NhZ2UpKTtcbiAgICB9XG4gIH07XG59XG4vLyMgc291cmNlTWFwcGluZ1VSTD1fcGFyc2VyLmpzLm1hcCIsIi8vIENvcHlyaWdodCAyMDE4LTIwMjUgdGhlIERlbm8gYXV0aG9ycy4gTUlUIGxpY2Vuc2UuXG4vLyBUaGlzIG1vZHVsZSBpcyBicm93c2VyIGNvbXBhdGlibGUuXG5pbXBvcnQgeyBwYXJzZXJGYWN0b3J5LCB0b21sIH0gZnJvbSBcIi4vX3BhcnNlci5qc1wiO1xuLyoqXG4gKiBQYXJzZXMgYSB7QGxpbmsgaHR0cHM6Ly90b21sLmlvIHwgVE9NTH0gc3RyaW5nIGludG8gYW4gb2JqZWN0LlxuICpcbiAqIEBleGFtcGxlIFVzYWdlXG4gKiBgYGB0c1xuICogaW1wb3J0IHsgcGFyc2UgfSBmcm9tIFwiQHN0ZC90b21sL3BhcnNlXCI7XG4gKiBpbXBvcnQgeyBhc3NlcnRFcXVhbHMgfSBmcm9tIFwiQHN0ZC9hc3NlcnRcIjtcbiAqXG4gKiBjb25zdCB0b21sU3RyaW5nID0gYHRpdGxlID0gXCJUT01MIEV4YW1wbGVcIlxuICogW293bmVyXVxuICogbmFtZSA9IFwiQWxpY2VcIlxuICogYmlvID0gXCJBbGljZSBpcyBhIHByb2dyYW1tZXIuXCJgO1xuICpcbiAqIGNvbnN0IG9iaiA9IHBhcnNlKHRvbWxTdHJpbmcpO1xuICogYXNzZXJ0RXF1YWxzKG9iaiwgeyB0aXRsZTogXCJUT01MIEV4YW1wbGVcIiwgb3duZXI6IHsgbmFtZTogXCJBbGljZVwiLCBiaW86IFwiQWxpY2UgaXMgYSBwcm9ncmFtbWVyLlwiIH0gfSk7XG4gKiBgYGBcbiAqIEBwYXJhbSB0b21sU3RyaW5nIFRPTUwgc3RyaW5nIHRvIGJlIHBhcnNlZC5cbiAqIEByZXR1cm5zIFRoZSBwYXJzZWQgSlMgb2JqZWN0LlxuICovIGV4cG9ydCBmdW5jdGlvbiBwYXJzZSh0b21sU3RyaW5nKSB7XG4gIHJldHVybiBwYXJzZXJGYWN0b3J5KHRvbWwpKHRvbWxTdHJpbmcpO1xufVxuLy8jIHNvdXJjZU1hcHBpbmdVUkw9cGFyc2UuanMubWFwIiwiaW1wb3J0IHsgY3JlYXRlUmVxdWlyZSB9IGZyb20gXCJub2RlOm1vZHVsZVwiO1xuaW1wb3J0IHsgaXNBYnNvbHV0ZSwgam9pbiwgcmVzb2x2ZSB9IGZyb20gXCJub2RlOnBhdGhcIjtcbmltcG9ydCB7IGZpbGVVUkxUb1BhdGggfSBmcm9tIFwibm9kZTp1cmxcIjtcbi8qKlxuKiBSZXNvbHZlIGFuIGFic29sdXRlIHBhdGggZnJvbSB7QGxpbmsgcm9vdH0sIGJ1dCBvbmx5XG4qIGlmIHtAbGluayBpbnB1dH0gaXNuJ3QgYWxyZWFkeSBhYnNvbHV0ZS5cbipcbiogQHBhcmFtIGlucHV0IFRoZSBwYXRoIHRvIHJlc29sdmUuXG4qIEBwYXJhbSByb290IFRoZSBiYXNlIHBhdGg7IGRlZmF1bHQgPSBwcm9jZXNzLmN3ZCgpXG4qIEByZXR1cm5zIFRoZSByZXNvbHZlZCBhYnNvbHV0ZSBwYXRoLlxuKi9cbmV4cG9ydCBmdW5jdGlvbiBhYnNvbHV0ZShpbnB1dCwgcm9vdCkge1xuXHRyZXR1cm4gaXNBYnNvbHV0ZShpbnB1dCkgPyBpbnB1dCA6IHJlc29sdmUocm9vdCB8fCBcIi5cIiwgaW5wdXQpO1xufVxuZXhwb3J0IGZ1bmN0aW9uIGZyb20ocm9vdCwgaWRlbnQsIHNpbGVudCkge1xuXHR0cnkge1xuXHRcdC8vIE5PVEU6IGRpcnMgbmVlZCBhIHRyYWlsaW5nIFwiL1wiIE9SIGZpbGVuYW1lLiBXaXRoIFwiL1wiIHJvdXRlLFxuXHRcdC8vIE5vZGUgYWRkcyBcIm5vb3AuanNcIiBhcyBtYWluIGZpbGUsIHNvIGp1c3QgZG8gXCJub29wLmpzXCIgYW55d2F5LlxuXHRcdGxldCByID0gcm9vdCBpbnN0YW5jZW9mIFVSTCB8fCByb290LnN0YXJ0c1dpdGgoXCJmaWxlOi8vXCIpID8gam9pbihmaWxlVVJMVG9QYXRoKHJvb3QpLCBcIm5vb3AuanNcIikgOiBqb2luKGFic29sdXRlKHJvb3QpLCBcIm5vb3AuanNcIik7XG5cdFx0cmV0dXJuIGNyZWF0ZVJlcXVpcmUocikucmVzb2x2ZShpZGVudCk7XG5cdH0gY2F0Y2ggKGVycikge1xuXHRcdGlmICghc2lsZW50KSB0aHJvdyBlcnI7XG5cdH1cbn1cbmV4cG9ydCBmdW5jdGlvbiBjd2QoaWRlbnQsIHNpbGVudCkge1xuXHRyZXR1cm4gZnJvbShyZXNvbHZlKCksIGlkZW50LCBzaWxlbnQpO1xufVxuIiwiaW1wb3J0IHsgZGlybmFtZSB9IGZyb20gXCJub2RlOnBhdGhcIjtcbmltcG9ydCB7IGFic29sdXRlIH0gZnJvbSBcImVtcGF0aGljL3Jlc29sdmVcIjtcbi8qKlxuKiBHZXQgYWxsIHBhcmVudCBkaXJlY3RvcmllcyBvZiB7QGxpbmsgYmFzZX0uXG4qIFN0b3BzIGFmdGVyIHtAbGluayBPcHRpb25zWydsYXN0J119IGlzIHByb2Nlc3NlZC5cbipcbiogQHJldHVybnMgQW4gYXJyYXkgb2YgYWJzb2x1dGUgcGF0aHMgb2YgYWxsIHBhcmVudCBkaXJlY3Rvcmllcy5cbiovXG5leHBvcnQgZnVuY3Rpb24gdXAoYmFzZSwgb3B0aW9ucykge1xuXHRsZXQgeyBsYXN0LCBjd2QgfSA9IG9wdGlvbnMgfHwge307XG5cdGxldCB0bXAgPSBhYnNvbHV0ZShiYXNlLCBjd2QpO1xuXHRsZXQgcm9vdCA9IGFic29sdXRlKGxhc3QgfHwgXCIvXCIsIGN3ZCk7XG5cdGxldCBwcmV2LCBhcnIgPSBbXTtcblx0d2hpbGUgKHByZXYgIT09IHJvb3QpIHtcblx0XHRhcnIucHVzaCh0bXApO1xuXHRcdHRtcCA9IGRpcm5hbWUocHJldiA9IHRtcCk7XG5cdFx0aWYgKHRtcCA9PT0gcHJldikgYnJlYWs7XG5cdH1cblx0cmV0dXJuIGFycjtcbn1cbiIsImltcG9ydCB7IGpvaW4gfSBmcm9tIFwibm9kZTpwYXRoXCI7XG5pbXBvcnQgeyBleGlzdHNTeW5jLCBzdGF0U3luYyB9IGZyb20gXCJub2RlOmZzXCI7XG5pbXBvcnQgKiBhcyB3YWxrIGZyb20gXCJlbXBhdGhpYy93YWxrXCI7XG4vKipcbiogRmluZCBhbiBpdGVtIGJ5IG5hbWUsIHdhbGtpbmcgcGFyZW50IGRpcmVjdG9yaWVzIHVudGlsIGZvdW5kLlxuKlxuKiBAcGFyYW0gbmFtZSBUaGUgaXRlbSBuYW1lIHRvIGZpbmQuXG4qIEByZXR1cm5zIFRoZSBhYnNvbHV0ZSBwYXRoIHRvIHRoZSBpdGVtLCBpZiBmb3VuZC5cbiovXG5leHBvcnQgZnVuY3Rpb24gdXAobmFtZSwgb3B0aW9ucykge1xuXHRsZXQgZGlyLCB0bXA7XG5cdGxldCBzdGFydCA9IG9wdGlvbnMgJiYgb3B0aW9ucy5jd2QgfHwgXCJcIjtcblx0Zm9yIChkaXIgb2Ygd2Fsay51cChzdGFydCwgb3B0aW9ucykpIHtcblx0XHR0bXAgPSBqb2luKGRpciwgbmFtZSk7XG5cdFx0aWYgKGV4aXN0c1N5bmModG1wKSkgcmV0dXJuIHRtcDtcblx0fVxufVxuLyoqXG4qIEdldCB0aGUgZmlyc3QgcGF0aCB0aGF0IG1hdGNoZXMgYW55IG9mIHRoZSBuYW1lcyBwcm92aWRlZC5cbipcbiogPiBbTk9URV1cbiogPiBUaGUgb3JkZXIgb2Yge0BsaW5rIG5hbWVzfSBpcyByZXNwZWN0ZWQuXG4qXG4qIEBwYXJhbSBuYW1lcyBUaGUgaXRlbSBuYW1lcyB0byBmaW5kLlxuKiBAcmV0dXJucyBUaGUgYWJzb2x1dGUgcGF0aCBvZiB0aGUgZmlyc3QgaXRlbSBmb3VuZCwgaWYgYW55LlxuKi9cbmV4cG9ydCBmdW5jdGlvbiBhbnkobmFtZXMsIG9wdGlvbnMpIHtcblx0bGV0IGRpciwgc3RhcnQgPSBvcHRpb25zICYmIG9wdGlvbnMuY3dkIHx8IFwiXCI7XG5cdGxldCBqID0gMCwgbGVuID0gbmFtZXMubGVuZ3RoLCB0bXA7XG5cdGZvciAoZGlyIG9mIHdhbGsudXAoc3RhcnQsIG9wdGlvbnMpKSB7XG5cdFx0Zm9yIChqID0gMDsgaiA8IGxlbjsgaisrKSB7XG5cdFx0XHR0bXAgPSBqb2luKGRpciwgbmFtZXNbal0pO1xuXHRcdFx0aWYgKGV4aXN0c1N5bmModG1wKSkgcmV0dXJuIHRtcDtcblx0XHR9XG5cdH1cbn1cbi8qKlxuKiBGaW5kIGEgZmlsZSBieSBuYW1lLCB3YWxraW5nIHBhcmVudCBkaXJlY3RvcmllcyB1bnRpbCBmb3VuZC5cbipcbiogPiBbTk9URV1cbiogPiBUaGlzIGZ1bmN0aW9uIG9ubHkgcmV0dXJucyBhIHZhbHVlIGZvciBmaWxlIG1hdGNoZXMuXG4qID4gQSBkaXJlY3RvcnkgbWF0Y2ggd2l0aCB0aGUgc2FtZSBuYW1lIHdpbGwgYmUgaWdub3JlZC5cbipcbiogQHBhcmFtIG5hbWUgVGhlIGZpbGUgbmFtZSB0byBmaW5kLlxuKiBAcmV0dXJucyBUaGUgYWJzb2x1dGUgcGF0aCB0byB0aGUgZmlsZSwgaWYgZm91bmQuXG4qL1xuZXhwb3J0IGZ1bmN0aW9uIGZpbGUobmFtZSwgb3B0aW9ucykge1xuXHRsZXQgZGlyLCB0bXA7XG5cdGxldCBzdGFydCA9IG9wdGlvbnMgJiYgb3B0aW9ucy5jd2QgfHwgXCJcIjtcblx0Zm9yIChkaXIgb2Ygd2Fsay51cChzdGFydCwgb3B0aW9ucykpIHtcblx0XHR0cnkge1xuXHRcdFx0dG1wID0gam9pbihkaXIsIG5hbWUpO1xuXHRcdFx0aWYgKHN0YXRTeW5jKHRtcCkuaXNGaWxlKCkpIHJldHVybiB0bXA7XG5cdFx0fSBjYXRjaCB7fVxuXHR9XG59XG4vKipcbiogRmluZCBhIGRpcmVjdG9yeSBieSBuYW1lLCB3YWxraW5nIHBhcmVudCBkaXJlY3RvcmllcyB1bnRpbCBmb3VuZC5cbipcbiogPiBbTk9URV1cbiogPiBUaGlzIGZ1bmN0aW9uIG9ubHkgcmV0dXJucyBhIHZhbHVlIGZvciBkaXJlY3RvcnkgbWF0Y2hlcy5cbiogPiBBIGZpbGUgbWF0Y2ggd2l0aCB0aGUgc2FtZSBuYW1lIHdpbGwgYmUgaWdub3JlZC5cbipcbiogQHBhcmFtIG5hbWUgVGhlIGRpcmVjdG9yeSBuYW1lIHRvIGZpbmQuXG4qIEByZXR1cm5zIFRoZSBhYnNvbHV0ZSBwYXRoIHRvIHRoZSBmaWxlLCBpZiBmb3VuZC5cbiovXG5leHBvcnQgZnVuY3Rpb24gZGlyKG5hbWUsIG9wdGlvbnMpIHtcblx0bGV0IGRpciwgdG1wO1xuXHRsZXQgc3RhcnQgPSBvcHRpb25zICYmIG9wdGlvbnMuY3dkIHx8IFwiXCI7XG5cdGZvciAoZGlyIG9mIHdhbGsudXAoc3RhcnQsIG9wdGlvbnMpKSB7XG5cdFx0dHJ5IHtcblx0XHRcdHRtcCA9IGpvaW4oZGlyLCBuYW1lKTtcblx0XHRcdGlmIChzdGF0U3luYyh0bXApLmlzRGlyZWN0b3J5KCkpIHJldHVybiB0bXA7XG5cdFx0fSBjYXRjaCB7fVxuXHR9XG59XG4iLCIvLyBUaGlzIGZpbGUgaXMgZ2VuZXJhdGVkIGJ5IGNvZGVnZW4vaW5kZXgudHNcbi8vIERvIG5vdCBlZGl0IHRoaXMgZmlsZSBtYW51YWxseVxuaW1wb3J0IHsgQ29tbWFuZCwgT3B0aW9uIH0gZnJvbSAnY2xpcGFuaW9uJ1xuXG5leHBvcnQgYWJzdHJhY3QgY2xhc3MgQmFzZVJlbmFtZUNvbW1hbmQgZXh0ZW5kcyBDb21tYW5kIHtcbiAgc3RhdGljIHBhdGhzID0gW1sncmVuYW1lJ11dXG5cbiAgc3RhdGljIHVzYWdlID0gQ29tbWFuZC5Vc2FnZSh7XG4gICAgZGVzY3JpcHRpb246ICdSZW5hbWUgdGhlIE5BUEktUlMgcHJvamVjdCcsXG4gIH0pXG5cbiAgY3dkID0gT3B0aW9uLlN0cmluZygnLS1jd2QnLCBwcm9jZXNzLmN3ZCgpLCB7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnVGhlIHdvcmtpbmcgZGlyZWN0b3J5IG9mIHdoZXJlIG5hcGkgY29tbWFuZCB3aWxsIGJlIGV4ZWN1dGVkIGluLCBhbGwgb3RoZXIgcGF0aHMgb3B0aW9ucyBhcmUgcmVsYXRpdmUgdG8gdGhpcyBwYXRoJyxcbiAgfSlcblxuICBjb25maWdQYXRoPzogc3RyaW5nID0gT3B0aW9uLlN0cmluZygnLS1jb25maWctcGF0aCwtYycsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1BhdGggdG8gYG5hcGlgIGNvbmZpZyBqc29uIGZpbGUnLFxuICB9KVxuXG4gIHBhY2thZ2VKc29uUGF0aCA9IE9wdGlvbi5TdHJpbmcoJy0tcGFja2FnZS1qc29uLXBhdGgnLCAncGFja2FnZS5qc29uJywge1xuICAgIGRlc2NyaXB0aW9uOiAnUGF0aCB0byBgcGFja2FnZS5qc29uYCcsXG4gIH0pXG5cbiAgbnBtRGlyID0gT3B0aW9uLlN0cmluZygnLS1ucG0tZGlyJywgJ25wbScsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1BhdGggdG8gdGhlIGZvbGRlciB3aGVyZSB0aGUgbnBtIHBhY2thZ2VzIHB1dCcsXG4gIH0pXG5cbiAgJCRuYW1lPzogc3RyaW5nID0gT3B0aW9uLlN0cmluZygnLS1uYW1lLC1uJywge1xuICAgIGRlc2NyaXB0aW9uOiAnVGhlIG5ldyBuYW1lIG9mIHRoZSBwcm9qZWN0JyxcbiAgfSlcblxuICBiaW5hcnlOYW1lPzogc3RyaW5nID0gT3B0aW9uLlN0cmluZygnLS1iaW5hcnktbmFtZSwtYicsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1RoZSBuZXcgYmluYXJ5IG5hbWUgKi5ub2RlIGZpbGVzJyxcbiAgfSlcblxuICBwYWNrYWdlTmFtZT86IHN0cmluZyA9IE9wdGlvbi5TdHJpbmcoJy0tcGFja2FnZS1uYW1lJywge1xuICAgIGRlc2NyaXB0aW9uOiAnVGhlIG5ldyBwYWNrYWdlIG5hbWUgb2YgdGhlIHByb2plY3QnLFxuICB9KVxuXG4gIG1hbmlmZXN0UGF0aCA9IE9wdGlvbi5TdHJpbmcoJy0tbWFuaWZlc3QtcGF0aCcsICdDYXJnby50b21sJywge1xuICAgIGRlc2NyaXB0aW9uOiAnUGF0aCB0byBgQ2FyZ28udG9tbGAnLFxuICB9KVxuXG4gIHJlcG9zaXRvcnk/OiBzdHJpbmcgPSBPcHRpb24uU3RyaW5nKCctLXJlcG9zaXRvcnknLCB7XG4gICAgZGVzY3JpcHRpb246ICdUaGUgbmV3IHJlcG9zaXRvcnkgb2YgdGhlIHByb2plY3QnLFxuICB9KVxuXG4gIGRlc2NyaXB0aW9uPzogc3RyaW5nID0gT3B0aW9uLlN0cmluZygnLS1kZXNjcmlwdGlvbicsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1RoZSBuZXcgZGVzY3JpcHRpb24gb2YgdGhlIHByb2plY3QnLFxuICB9KVxuXG4gIGdldE9wdGlvbnMoKSB7XG4gICAgcmV0dXJuIHtcbiAgICAgIGN3ZDogdGhpcy5jd2QsXG4gICAgICBjb25maWdQYXRoOiB0aGlzLmNvbmZpZ1BhdGgsXG4gICAgICBwYWNrYWdlSnNvblBhdGg6IHRoaXMucGFja2FnZUpzb25QYXRoLFxuICAgICAgbnBtRGlyOiB0aGlzLm5wbURpcixcbiAgICAgIG5hbWU6IHRoaXMuJCRuYW1lLFxuICAgICAgYmluYXJ5TmFtZTogdGhpcy5iaW5hcnlOYW1lLFxuICAgICAgcGFja2FnZU5hbWU6IHRoaXMucGFja2FnZU5hbWUsXG4gICAgICBtYW5pZmVzdFBhdGg6IHRoaXMubWFuaWZlc3RQYXRoLFxuICAgICAgcmVwb3NpdG9yeTogdGhpcy5yZXBvc2l0b3J5LFxuICAgICAgZGVzY3JpcHRpb246IHRoaXMuZGVzY3JpcHRpb24sXG4gICAgfVxuICB9XG59XG5cbi8qKlxuICogUmVuYW1lIHRoZSBOQVBJLVJTIHByb2plY3RcbiAqL1xuZXhwb3J0IGludGVyZmFjZSBSZW5hbWVPcHRpb25zIHtcbiAgLyoqXG4gICAqIFRoZSB3b3JraW5nIGRpcmVjdG9yeSBvZiB3aGVyZSBuYXBpIGNvbW1hbmQgd2lsbCBiZSBleGVjdXRlZCBpbiwgYWxsIG90aGVyIHBhdGhzIG9wdGlvbnMgYXJlIHJlbGF0aXZlIHRvIHRoaXMgcGF0aFxuICAgKlxuICAgKiBAZGVmYXVsdCBwcm9jZXNzLmN3ZCgpXG4gICAqL1xuICBjd2Q/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gYG5hcGlgIGNvbmZpZyBqc29uIGZpbGVcbiAgICovXG4gIGNvbmZpZ1BhdGg/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gYHBhY2thZ2UuanNvbmBcbiAgICpcbiAgICogQGRlZmF1bHQgJ3BhY2thZ2UuanNvbidcbiAgICovXG4gIHBhY2thZ2VKc29uUGF0aD86IHN0cmluZ1xuICAvKipcbiAgICogUGF0aCB0byB0aGUgZm9sZGVyIHdoZXJlIHRoZSBucG0gcGFja2FnZXMgcHV0XG4gICAqXG4gICAqIEBkZWZhdWx0ICducG0nXG4gICAqL1xuICBucG1EaXI/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFRoZSBuZXcgbmFtZSBvZiB0aGUgcHJvamVjdFxuICAgKi9cbiAgbmFtZT86IHN0cmluZ1xuICAvKipcbiAgICogVGhlIG5ldyBiaW5hcnkgbmFtZSAqLm5vZGUgZmlsZXNcbiAgICovXG4gIGJpbmFyeU5hbWU/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFRoZSBuZXcgcGFja2FnZSBuYW1lIG9mIHRoZSBwcm9qZWN0XG4gICAqL1xuICBwYWNrYWdlTmFtZT86IHN0cmluZ1xuICAvKipcbiAgICogUGF0aCB0byBgQ2FyZ28udG9tbGBcbiAgICpcbiAgICogQGRlZmF1bHQgJ0NhcmdvLnRvbWwnXG4gICAqL1xuICBtYW5pZmVzdFBhdGg/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFRoZSBuZXcgcmVwb3NpdG9yeSBvZiB0aGUgcHJvamVjdFxuICAgKi9cbiAgcmVwb3NpdG9yeT86IHN0cmluZ1xuICAvKipcbiAgICogVGhlIG5ldyBkZXNjcmlwdGlvbiBvZiB0aGUgcHJvamVjdFxuICAgKi9cbiAgZGVzY3JpcHRpb24/OiBzdHJpbmdcbn1cblxuZXhwb3J0IGZ1bmN0aW9uIGFwcGx5RGVmYXVsdFJlbmFtZU9wdGlvbnMob3B0aW9uczogUmVuYW1lT3B0aW9ucykge1xuICByZXR1cm4ge1xuICAgIGN3ZDogcHJvY2Vzcy5jd2QoKSxcbiAgICBwYWNrYWdlSnNvblBhdGg6ICdwYWNrYWdlLmpzb24nLFxuICAgIG5wbURpcjogJ25wbScsXG4gICAgbWFuaWZlc3RQYXRoOiAnQ2FyZ28udG9tbCcsXG4gICAgLi4ub3B0aW9ucyxcbiAgfVxufVxuIiwiaW1wb3J0IHsgZXhpc3RzU3luYyB9IGZyb20gJ25vZGU6ZnMnXG5pbXBvcnQgeyByZW5hbWUgfSBmcm9tICdub2RlOmZzL3Byb21pc2VzJ1xuaW1wb3J0IHsgcmVzb2x2ZSwgam9pbiB9IGZyb20gJ25vZGU6cGF0aCdcblxuaW1wb3J0IHsgcGFyc2UgYXMgcGFyc2VUb21sLCBzdHJpbmdpZnkgYXMgc3RyaW5naWZ5VG9tbCB9IGZyb20gJ0BzdGQvdG9tbCdcbmltcG9ydCB7IGxvYWQgYXMgeWFtbFBhcnNlLCBkdW1wIGFzIHlhbWxTdHJpbmdpZnkgfSBmcm9tICdqcy15YW1sJ1xuaW1wb3J0IHsgaXNOaWwsIG1lcmdlLCBvbWl0QnksIHBpY2sgfSBmcm9tICdlcy10b29sa2l0J1xuaW1wb3J0ICogYXMgZmluZCBmcm9tICdlbXBhdGhpYy9maW5kJ1xuXG5pbXBvcnQgeyBhcHBseURlZmF1bHRSZW5hbWVPcHRpb25zLCB0eXBlIFJlbmFtZU9wdGlvbnMgfSBmcm9tICcuLi9kZWYvcmVuYW1lLmpzJ1xuaW1wb3J0IHsgcmVhZENvbmZpZywgcmVhZEZpbGVBc3luYywgd3JpdGVGaWxlQXN5bmMgfSBmcm9tICcuLi91dGlscy9pbmRleC5qcydcblxuZXhwb3J0IGFzeW5jIGZ1bmN0aW9uIHJlbmFtZVByb2plY3QodXNlck9wdGlvbnM6IFJlbmFtZU9wdGlvbnMpIHtcbiAgY29uc3Qgb3B0aW9ucyA9IGFwcGx5RGVmYXVsdFJlbmFtZU9wdGlvbnModXNlck9wdGlvbnMpXG4gIGNvbnN0IG5hcGlDb25maWcgPSBhd2FpdCByZWFkQ29uZmlnKG9wdGlvbnMpXG4gIGNvbnN0IG9sZE5hbWUgPSBuYXBpQ29uZmlnLmJpbmFyeU5hbWVcblxuICBjb25zdCBwYWNrYWdlSnNvblBhdGggPSByZXNvbHZlKG9wdGlvbnMuY3dkLCBvcHRpb25zLnBhY2thZ2VKc29uUGF0aClcbiAgY29uc3QgY2FyZ29Ub21sUGF0aCA9IHJlc29sdmUob3B0aW9ucy5jd2QsIG9wdGlvbnMubWFuaWZlc3RQYXRoKVxuXG4gIGNvbnN0IHBhY2thZ2VKc29uQ29udGVudCA9IGF3YWl0IHJlYWRGaWxlQXN5bmMocGFja2FnZUpzb25QYXRoLCAndXRmOCcpXG4gIGNvbnN0IHBhY2thZ2VKc29uRGF0YSA9IEpTT04ucGFyc2UocGFja2FnZUpzb25Db250ZW50KVxuXG4gIG1lcmdlKFxuICAgIG1lcmdlKFxuICAgICAgcGFja2FnZUpzb25EYXRhLFxuICAgICAgb21pdEJ5KFxuICAgICAgICAvLyBAdHMtZXhwZWN0LWVycm9yIG1pc3NpbmcgZmllbGRzOiBhdXRob3IgYW5kIGxpY2Vuc2VcbiAgICAgICAgcGljayhvcHRpb25zLCBbJ25hbWUnLCAnZGVzY3JpcHRpb24nLCAnYXV0aG9yJywgJ2xpY2Vuc2UnXSksXG4gICAgICAgIGlzTmlsLFxuICAgICAgKSxcbiAgICApLFxuICAgIHtcbiAgICAgIG5hcGk6IG9taXRCeShcbiAgICAgICAge1xuICAgICAgICAgIGJpbmFyeU5hbWU6IG9wdGlvbnMuYmluYXJ5TmFtZSxcbiAgICAgICAgICBwYWNrYWdlTmFtZTogb3B0aW9ucy5wYWNrYWdlTmFtZSxcbiAgICAgICAgfSxcbiAgICAgICAgaXNOaWwsXG4gICAgICApLFxuICAgIH0sXG4gIClcblxuICBpZiAob3B0aW9ucy5jb25maWdQYXRoKSB7XG4gICAgY29uc3QgY29uZmlnUGF0aCA9IHJlc29sdmUob3B0aW9ucy5jd2QsIG9wdGlvbnMuY29uZmlnUGF0aClcbiAgICBjb25zdCBjb25maWdDb250ZW50ID0gYXdhaXQgcmVhZEZpbGVBc3luYyhjb25maWdQYXRoLCAndXRmOCcpXG4gICAgY29uc3QgY29uZmlnRGF0YSA9IEpTT04ucGFyc2UoY29uZmlnQ29udGVudClcbiAgICBjb25maWdEYXRhLmJpbmFyeU5hbWUgPSBvcHRpb25zLmJpbmFyeU5hbWVcbiAgICBjb25maWdEYXRhLnBhY2thZ2VOYW1lID0gb3B0aW9ucy5wYWNrYWdlTmFtZVxuICAgIGF3YWl0IHdyaXRlRmlsZUFzeW5jKGNvbmZpZ1BhdGgsIEpTT04uc3RyaW5naWZ5KGNvbmZpZ0RhdGEsIG51bGwsIDIpKVxuICB9XG5cbiAgYXdhaXQgd3JpdGVGaWxlQXN5bmMoXG4gICAgcGFja2FnZUpzb25QYXRoLFxuICAgIEpTT04uc3RyaW5naWZ5KHBhY2thZ2VKc29uRGF0YSwgbnVsbCwgMiksXG4gIClcblxuICBjb25zdCB0b21sQ29udGVudCA9IGF3YWl0IHJlYWRGaWxlQXN5bmMoY2FyZ29Ub21sUGF0aCwgJ3V0ZjgnKVxuICBjb25zdCBjYXJnb1RvbWwgPSBwYXJzZVRvbWwodG9tbENvbnRlbnQpIGFzIGFueVxuXG4gIC8vIFVwZGF0ZSB0aGUgcGFja2FnZSBuYW1lXG4gIGlmIChjYXJnb1RvbWwucGFja2FnZSAmJiBvcHRpb25zLmJpbmFyeU5hbWUpIHtcbiAgICAvLyBTYW5pdGl6ZSB0aGUgYmluYXJ5IG5hbWUgZm9yIFJ1c3QgcGFja2FnZSBuYW1pbmcgY29udmVudGlvbnNcbiAgICBjb25zdCBzYW5pdGl6ZWROYW1lID0gb3B0aW9ucy5iaW5hcnlOYW1lXG4gICAgICAucmVwbGFjZSgnQCcsICcnKVxuICAgICAgLnJlcGxhY2UoJy8nLCAnXycpXG4gICAgICAucmVwbGFjZSgvLS9nLCAnXycpXG4gICAgICAudG9Mb3dlckNhc2UoKVxuICAgIGNhcmdvVG9tbC5wYWNrYWdlLm5hbWUgPSBzYW5pdGl6ZWROYW1lXG4gIH1cblxuICAvLyBTdHJpbmdpZnkgdGhlIHVwZGF0ZWQgVE9NTFxuICBjb25zdCB1cGRhdGVkVG9tbENvbnRlbnQgPSBzdHJpbmdpZnlUb21sKGNhcmdvVG9tbClcblxuICBhd2FpdCB3cml0ZUZpbGVBc3luYyhjYXJnb1RvbWxQYXRoLCB1cGRhdGVkVG9tbENvbnRlbnQpXG4gIGlmIChvbGROYW1lICE9PSBvcHRpb25zLmJpbmFyeU5hbWUpIHtcbiAgICBjb25zdCBnaXRodWJBY3Rpb25zUGF0aCA9IGZpbmQuZGlyKCcuZ2l0aHViJywge1xuICAgICAgY3dkOiBvcHRpb25zLmN3ZCxcbiAgICB9KVxuICAgIGlmIChnaXRodWJBY3Rpb25zUGF0aCkge1xuICAgICAgY29uc3QgZ2l0aHViQWN0aW9uc0NJWW1sUGF0aCA9IGpvaW4oXG4gICAgICAgIGdpdGh1YkFjdGlvbnNQYXRoLFxuICAgICAgICAnd29ya2Zsb3dzJyxcbiAgICAgICAgJ0NJLnltbCcsXG4gICAgICApXG4gICAgICBpZiAoZXhpc3RzU3luYyhnaXRodWJBY3Rpb25zQ0lZbWxQYXRoKSkge1xuICAgICAgICBjb25zdCBnaXRodWJBY3Rpb25zQ29udGVudCA9IGF3YWl0IHJlYWRGaWxlQXN5bmMoXG4gICAgICAgICAgZ2l0aHViQWN0aW9uc0NJWW1sUGF0aCxcbiAgICAgICAgICAndXRmOCcsXG4gICAgICAgIClcbiAgICAgICAgY29uc3QgZ2l0aHViQWN0aW9uc0RhdGEgPSB5YW1sUGFyc2UoZ2l0aHViQWN0aW9uc0NvbnRlbnQpIGFzIGFueVxuICAgICAgICBpZiAoZ2l0aHViQWN0aW9uc0RhdGEuZW52Py5BUFBfTkFNRSkge1xuICAgICAgICAgIGdpdGh1YkFjdGlvbnNEYXRhLmVudi5BUFBfTkFNRSA9IG9wdGlvbnMuYmluYXJ5TmFtZVxuICAgICAgICAgIGF3YWl0IHdyaXRlRmlsZUFzeW5jKFxuICAgICAgICAgICAgZ2l0aHViQWN0aW9uc0NJWW1sUGF0aCxcbiAgICAgICAgICAgIHlhbWxTdHJpbmdpZnkoZ2l0aHViQWN0aW9uc0RhdGEsIHtcbiAgICAgICAgICAgICAgbGluZVdpZHRoOiAtMSxcbiAgICAgICAgICAgICAgbm9SZWZzOiB0cnVlLFxuICAgICAgICAgICAgICBzb3J0S2V5czogZmFsc2UsXG4gICAgICAgICAgICB9KSxcbiAgICAgICAgICApXG4gICAgICAgIH1cbiAgICAgIH1cbiAgICB9XG4gICAgY29uc3Qgb2xkV2FzaUJyb3dzZXJCaW5kaW5nUGF0aCA9IGpvaW4oXG4gICAgICBvcHRpb25zLmN3ZCxcbiAgICAgIGAke29sZE5hbWV9Lndhc2ktYnJvd3Nlci5qc2AsXG4gICAgKVxuICAgIGlmIChleGlzdHNTeW5jKG9sZFdhc2lCcm93c2VyQmluZGluZ1BhdGgpKSB7XG4gICAgICBhd2FpdCByZW5hbWUoXG4gICAgICAgIG9sZFdhc2lCcm93c2VyQmluZGluZ1BhdGgsXG4gICAgICAgIGpvaW4ob3B0aW9ucy5jd2QsIGAke29wdGlvbnMuYmluYXJ5TmFtZX0ud2FzaS1icm93c2VyLmpzYCksXG4gICAgICApXG4gICAgfVxuICAgIGNvbnN0IG9sZFdhc2lCaW5kaW5nUGF0aCA9IGpvaW4ob3B0aW9ucy5jd2QsIGAke29sZE5hbWV9Lndhc2kuY2pzYClcbiAgICBpZiAoZXhpc3RzU3luYyhvbGRXYXNpQmluZGluZ1BhdGgpKSB7XG4gICAgICBhd2FpdCByZW5hbWUoXG4gICAgICAgIG9sZFdhc2lCaW5kaW5nUGF0aCxcbiAgICAgICAgam9pbihvcHRpb25zLmN3ZCwgYCR7b3B0aW9ucy5iaW5hcnlOYW1lfS53YXNpLmNqc2ApLFxuICAgICAgKVxuICAgIH1cbiAgICBjb25zdCBnaXRBdHRyaWJ1dGVzUGF0aCA9IGpvaW4ob3B0aW9ucy5jd2QsICcuZ2l0YXR0cmlidXRlcycpXG4gICAgaWYgKGV4aXN0c1N5bmMoZ2l0QXR0cmlidXRlc1BhdGgpKSB7XG4gICAgICBjb25zdCBnaXRBdHRyaWJ1dGVzQ29udGVudCA9IGF3YWl0IHJlYWRGaWxlQXN5bmMoXG4gICAgICAgIGdpdEF0dHJpYnV0ZXNQYXRoLFxuICAgICAgICAndXRmOCcsXG4gICAgICApXG4gICAgICBjb25zdCBnaXRBdHRyaWJ1dGVzRGF0YSA9IGdpdEF0dHJpYnV0ZXNDb250ZW50XG4gICAgICAgIC5zcGxpdCgnXFxuJylcbiAgICAgICAgLm1hcCgobGluZSkgPT4ge1xuICAgICAgICAgIHJldHVybiBsaW5lXG4gICAgICAgICAgICAucmVwbGFjZShcbiAgICAgICAgICAgICAgYCR7b2xkTmFtZX0ud2FzaS1icm93c2VyLmpzYCxcbiAgICAgICAgICAgICAgYCR7b3B0aW9ucy5iaW5hcnlOYW1lfS53YXNpLWJyb3dzZXIuanNgLFxuICAgICAgICAgICAgKVxuICAgICAgICAgICAgLnJlcGxhY2UoYCR7b2xkTmFtZX0ud2FzaS5janNgLCBgJHtvcHRpb25zLmJpbmFyeU5hbWV9Lndhc2kuY2pzYClcbiAgICAgICAgfSlcbiAgICAgICAgLmpvaW4oJ1xcbicpXG4gICAgICBhd2FpdCB3cml0ZUZpbGVBc3luYyhnaXRBdHRyaWJ1dGVzUGF0aCwgZ2l0QXR0cmlidXRlc0RhdGEpXG4gICAgfVxuICB9XG59XG4iLCJpbXBvcnQgeyBleGVjLCBleGVjU3luYyB9IGZyb20gJ25vZGU6Y2hpbGRfcHJvY2VzcydcbmltcG9ydCB7IGV4aXN0c1N5bmMgfSBmcm9tICdub2RlOmZzJ1xuaW1wb3J0IHsgaG9tZWRpciB9IGZyb20gJ25vZGU6b3MnXG5pbXBvcnQgcGF0aCBmcm9tICdub2RlOnBhdGgnXG5pbXBvcnQgeyBwcm9taXNlcyBhcyBmcyB9IGZyb20gJ25vZGU6ZnMnXG5cbmltcG9ydCB7IGxvYWQgYXMgeWFtbExvYWQsIGR1bXAgYXMgeWFtbER1bXAgfSBmcm9tICdqcy15YW1sJ1xuXG5pbXBvcnQge1xuICBhcHBseURlZmF1bHROZXdPcHRpb25zLFxuICB0eXBlIE5ld09wdGlvbnMgYXMgUmF3TmV3T3B0aW9ucyxcbn0gZnJvbSAnLi4vZGVmL25ldy5qcydcbmltcG9ydCB7XG4gIEFWQUlMQUJMRV9UQVJHRVRTLFxuICBkZWJ1Z0ZhY3RvcnksXG4gIERFRkFVTFRfVEFSR0VUUyxcbiAgbWtkaXJBc3luYyxcbiAgcmVhZGRpckFzeW5jLFxuICBzdGF0QXN5bmMsXG4gIHR5cGUgU3VwcG9ydGVkUGFja2FnZU1hbmFnZXIsXG59IGZyb20gJy4uL3V0aWxzL2luZGV4LmpzJ1xuaW1wb3J0IHsgbmFwaUVuZ2luZVJlcXVpcmVtZW50IH0gZnJvbSAnLi4vdXRpbHMvdmVyc2lvbi5qcydcbmltcG9ydCB7IHJlbmFtZVByb2plY3QgfSBmcm9tICcuL3JlbmFtZS5qcydcblxuLy8gVGVtcGxhdGUgaW1wb3J0cyByZW1vdmVkIGFzIHdlJ3JlIG5vdyB1c2luZyBleHRlcm5hbCB0ZW1wbGF0ZXNcblxuY29uc3QgZGVidWcgPSBkZWJ1Z0ZhY3RvcnkoJ25ldycpXG5cbnR5cGUgTmV3T3B0aW9ucyA9IFJlcXVpcmVkPFJhd05ld09wdGlvbnM+XG5cbmNvbnN0IFRFTVBMQVRFX1JFUE9TID0ge1xuICB5YXJuOiAnaHR0cHM6Ly9naXRodWIuY29tL25hcGktcnMvcGFja2FnZS10ZW1wbGF0ZScsXG4gIHBucG06ICdodHRwczovL2dpdGh1Yi5jb20vbmFwaS1ycy9wYWNrYWdlLXRlbXBsYXRlLXBucG0nLFxufSBhcyBjb25zdFxuXG5hc3luYyBmdW5jdGlvbiBjaGVja0dpdENvbW1hbmQoKTogUHJvbWlzZTxib29sZWFuPiB7XG4gIHRyeSB7XG4gICAgYXdhaXQgbmV3IFByb21pc2UoKHJlc29sdmUpID0+IHtcbiAgICAgIGNvbnN0IGNwID0gZXhlYygnZ2l0IC0tdmVyc2lvbicpXG4gICAgICBjcC5vbignZXJyb3InLCAoKSA9PiB7XG4gICAgICAgIHJlc29sdmUoZmFsc2UpXG4gICAgICB9KVxuICAgICAgY3Aub24oJ2V4aXQnLCAoY29kZSkgPT4ge1xuICAgICAgICBpZiAoY29kZSA9PT0gMCkge1xuICAgICAgICAgIHJlc29sdmUodHJ1ZSlcbiAgICAgICAgfSBlbHNlIHtcbiAgICAgICAgICByZXNvbHZlKGZhbHNlKVxuICAgICAgICB9XG4gICAgICB9KVxuICAgIH0pXG4gICAgcmV0dXJuIHRydWVcbiAgfSBjYXRjaCB7XG4gICAgcmV0dXJuIGZhbHNlXG4gIH1cbn1cblxuYXN5bmMgZnVuY3Rpb24gZW5zdXJlQ2FjaGVEaXIoXG4gIHBhY2thZ2VNYW5hZ2VyOiBTdXBwb3J0ZWRQYWNrYWdlTWFuYWdlcixcbik6IFByb21pc2U8c3RyaW5nPiB7XG4gIGNvbnN0IGNhY2hlRGlyID0gcGF0aC5qb2luKGhvbWVkaXIoKSwgJy5uYXBpLXJzJywgJ3RlbXBsYXRlJywgcGFja2FnZU1hbmFnZXIpXG4gIGF3YWl0IG1rZGlyQXN5bmMoY2FjaGVEaXIsIHsgcmVjdXJzaXZlOiB0cnVlIH0pXG4gIHJldHVybiBjYWNoZURpclxufVxuXG5hc3luYyBmdW5jdGlvbiBkb3dubG9hZFRlbXBsYXRlKFxuICBwYWNrYWdlTWFuYWdlcjogU3VwcG9ydGVkUGFja2FnZU1hbmFnZXIsXG4gIGNhY2hlRGlyOiBzdHJpbmcsXG4pOiBQcm9taXNlPHZvaWQ+IHtcbiAgY29uc3QgcmVwb1VybCA9IFRFTVBMQVRFX1JFUE9TW3BhY2thZ2VNYW5hZ2VyXVxuICBjb25zdCB0ZW1wbGF0ZVBhdGggPSBwYXRoLmpvaW4oY2FjaGVEaXIsICdyZXBvJylcblxuICBpZiAoZXhpc3RzU3luYyh0ZW1wbGF0ZVBhdGgpKSB7XG4gICAgZGVidWcoYFRlbXBsYXRlIGNhY2hlIGZvdW5kIGF0ICR7dGVtcGxhdGVQYXRofSwgdXBkYXRpbmcuLi5gKVxuICAgIHRyeSB7XG4gICAgICAvLyBGZXRjaCBsYXRlc3QgY2hhbmdlcyBhbmQgcmVzZXQgdG8gcmVtb3RlXG4gICAgICBhd2FpdCBuZXcgUHJvbWlzZTx2b2lkPigocmVzb2x2ZSwgcmVqZWN0KSA9PiB7XG4gICAgICAgIGNvbnN0IGNwID0gZXhlYygnZ2l0IGZldGNoIG9yaWdpbicsIHsgY3dkOiB0ZW1wbGF0ZVBhdGggfSlcbiAgICAgICAgY3Aub24oJ2Vycm9yJywgcmVqZWN0KVxuICAgICAgICBjcC5vbignZXhpdCcsIChjb2RlKSA9PiB7XG4gICAgICAgICAgaWYgKGNvZGUgPT09IDApIHtcbiAgICAgICAgICAgIHJlc29sdmUoKVxuICAgICAgICAgIH0gZWxzZSB7XG4gICAgICAgICAgICByZWplY3QoXG4gICAgICAgICAgICAgIG5ldyBFcnJvcihcbiAgICAgICAgICAgICAgICBgRmFpbGVkIHRvIGZldGNoIGxhdGVzdCBjaGFuZ2VzLCBnaXQgcHJvY2VzcyBleGl0ZWQgd2l0aCBjb2RlICR7Y29kZX1gLFxuICAgICAgICAgICAgICApLFxuICAgICAgICAgICAgKVxuICAgICAgICAgIH1cbiAgICAgICAgfSlcbiAgICAgIH0pXG4gICAgICBleGVjU3luYygnZ2l0IHJlc2V0IC0taGFyZCBvcmlnaW4vbWFpbicsIHtcbiAgICAgICAgY3dkOiB0ZW1wbGF0ZVBhdGgsXG4gICAgICAgIHN0ZGlvOiAnaWdub3JlJyxcbiAgICAgIH0pXG4gICAgICBkZWJ1ZygnVGVtcGxhdGUgdXBkYXRlZCBzdWNjZXNzZnVsbHknKVxuICAgIH0gY2F0Y2ggKGVycm9yKSB7XG4gICAgICBkZWJ1ZyhgRmFpbGVkIHRvIHVwZGF0ZSB0ZW1wbGF0ZTogJHtlcnJvcn1gKVxuICAgICAgdGhyb3cgbmV3IEVycm9yKGBGYWlsZWQgdG8gdXBkYXRlIHRlbXBsYXRlIGZyb20gJHtyZXBvVXJsfTogJHtlcnJvcn1gKVxuICAgIH1cbiAgfSBlbHNlIHtcbiAgICBkZWJ1ZyhgQ2xvbmluZyB0ZW1wbGF0ZSBmcm9tICR7cmVwb1VybH0uLi5gKVxuICAgIHRyeSB7XG4gICAgICBleGVjU3luYyhgZ2l0IGNsb25lICR7cmVwb1VybH0gcmVwb2AsIHsgY3dkOiBjYWNoZURpciwgc3RkaW86ICdpbmhlcml0JyB9KVxuICAgICAgZGVidWcoJ1RlbXBsYXRlIGNsb25lZCBzdWNjZXNzZnVsbHknKVxuICAgIH0gY2F0Y2ggKGVycm9yKSB7XG4gICAgICB0aHJvdyBuZXcgRXJyb3IoYEZhaWxlZCB0byBjbG9uZSB0ZW1wbGF0ZSBmcm9tICR7cmVwb1VybH06ICR7ZXJyb3J9YClcbiAgICB9XG4gIH1cbn1cblxuYXN5bmMgZnVuY3Rpb24gY29weURpcmVjdG9yeShcbiAgc3JjOiBzdHJpbmcsXG4gIGRlc3Q6IHN0cmluZyxcbiAgaW5jbHVkZVdhc2lCaW5kaW5nczogYm9vbGVhbixcbik6IFByb21pc2U8dm9pZD4ge1xuICBhd2FpdCBta2RpckFzeW5jKGRlc3QsIHsgcmVjdXJzaXZlOiB0cnVlIH0pXG4gIGNvbnN0IGVudHJpZXMgPSBhd2FpdCBmcy5yZWFkZGlyKHNyYywgeyB3aXRoRmlsZVR5cGVzOiB0cnVlIH0pXG5cbiAgZm9yIChjb25zdCBlbnRyeSBvZiBlbnRyaWVzKSB7XG4gICAgY29uc3Qgc3JjUGF0aCA9IHBhdGguam9pbihzcmMsIGVudHJ5Lm5hbWUpXG4gICAgY29uc3QgZGVzdFBhdGggPSBwYXRoLmpvaW4oZGVzdCwgZW50cnkubmFtZSlcblxuICAgIC8vIFNraXAgLmdpdCBkaXJlY3RvcnlcbiAgICBpZiAoZW50cnkubmFtZSA9PT0gJy5naXQnKSB7XG4gICAgICBjb250aW51ZVxuICAgIH1cblxuICAgIGlmIChlbnRyeS5pc0RpcmVjdG9yeSgpKSB7XG4gICAgICBhd2FpdCBjb3B5RGlyZWN0b3J5KHNyY1BhdGgsIGRlc3RQYXRoLCBpbmNsdWRlV2FzaUJpbmRpbmdzKVxuICAgIH0gZWxzZSB7XG4gICAgICBpZiAoXG4gICAgICAgICFpbmNsdWRlV2FzaUJpbmRpbmdzICYmXG4gICAgICAgIChlbnRyeS5uYW1lLmVuZHNXaXRoKCcud2FzaS1icm93c2VyLmpzJykgfHxcbiAgICAgICAgICBlbnRyeS5uYW1lLmVuZHNXaXRoKCcud2FzaS5janMnKSB8fFxuICAgICAgICAgIGVudHJ5Lm5hbWUuZW5kc1dpdGgoJ3dhc2ktd29ya2VyLmJyb3dzZXIubWpzICcpIHx8XG4gICAgICAgICAgZW50cnkubmFtZS5lbmRzV2l0aCgnd2FzaS13b3JrZXIubWpzJykgfHxcbiAgICAgICAgICBlbnRyeS5uYW1lLmVuZHNXaXRoKCdicm93c2VyLmpzJykpXG4gICAgICApIHtcbiAgICAgICAgY29udGludWVcbiAgICAgIH1cbiAgICAgIGF3YWl0IGZzLmNvcHlGaWxlKHNyY1BhdGgsIGRlc3RQYXRoKVxuICAgIH1cbiAgfVxufVxuXG5hc3luYyBmdW5jdGlvbiBmaWx0ZXJUYXJnZXRzSW5QYWNrYWdlSnNvbihcbiAgZmlsZVBhdGg6IHN0cmluZyxcbiAgZW5hYmxlZFRhcmdldHM6IHN0cmluZ1tdLFxuKTogUHJvbWlzZTx2b2lkPiB7XG4gIGNvbnN0IGNvbnRlbnQgPSBhd2FpdCBmcy5yZWFkRmlsZShmaWxlUGF0aCwgJ3V0Zi04JylcbiAgY29uc3QgcGFja2FnZUpzb24gPSBKU09OLnBhcnNlKGNvbnRlbnQpXG5cbiAgLy8gRmlsdGVyIG5hcGkudGFyZ2V0c1xuICBpZiAocGFja2FnZUpzb24ubmFwaT8udGFyZ2V0cykge1xuICAgIHBhY2thZ2VKc29uLm5hcGkudGFyZ2V0cyA9IHBhY2thZ2VKc29uLm5hcGkudGFyZ2V0cy5maWx0ZXIoXG4gICAgICAodGFyZ2V0OiBzdHJpbmcpID0+IGVuYWJsZWRUYXJnZXRzLmluY2x1ZGVzKHRhcmdldCksXG4gICAgKVxuICB9XG5cbiAgYXdhaXQgZnMud3JpdGVGaWxlKGZpbGVQYXRoLCBKU09OLnN0cmluZ2lmeShwYWNrYWdlSnNvbiwgbnVsbCwgMikgKyAnXFxuJylcbn1cblxuYXN5bmMgZnVuY3Rpb24gZmlsdGVyVGFyZ2V0c0luR2l0aHViQWN0aW9ucyhcbiAgZmlsZVBhdGg6IHN0cmluZyxcbiAgZW5hYmxlZFRhcmdldHM6IHN0cmluZ1tdLFxuKTogUHJvbWlzZTx2b2lkPiB7XG4gIGNvbnN0IGNvbnRlbnQgPSBhd2FpdCBmcy5yZWFkRmlsZShmaWxlUGF0aCwgJ3V0Zi04JylcbiAgY29uc3QgeWFtbCA9IHlhbWxMb2FkKGNvbnRlbnQpIGFzIGFueVxuXG4gIGNvbnN0IG1hY09TQW5kV2luZG93c1RhcmdldHMgPSBuZXcgU2V0KFtcbiAgICAneDg2XzY0LXBjLXdpbmRvd3MtbXN2YycsXG4gICAgJ3g4Nl82NC1wYy13aW5kb3dzLWdudScsXG4gICAgJ2FhcmNoNjQtcGMtd2luZG93cy1tc3ZjJyxcbiAgICAneDg2XzY0LWFwcGxlLWRhcndpbicsXG4gIF0pXG5cbiAgY29uc3QgbGludXhUYXJnZXRzID0gbmV3IFNldChbXG4gICAgJ3g4Nl82NC11bmtub3duLWxpbnV4LWdudScsXG4gICAgJ3g4Nl82NC11bmtub3duLWxpbnV4LW11c2wnLFxuICAgICdhYXJjaDY0LXVua25vd24tbGludXgtZ251JyxcbiAgICAnYWFyY2g2NC11bmtub3duLWxpbnV4LW11c2wnLFxuICAgICdhcm12Ny11bmtub3duLWxpbnV4LWdudWVhYmloZicsXG4gICAgJ2FybXY3LXVua25vd24tbGludXgtbXVzbGVhYmloZicsXG4gICAgJ2xvb25nYXJjaDY0LXVua25vd24tbGludXgtZ251JyxcbiAgICAncmlzY3Y2NGdjLXVua25vd24tbGludXgtZ251JyxcbiAgICAncG93ZXJwYzY0bGUtdW5rbm93bi1saW51eC1nbnUnLFxuICAgICdzMzkweC11bmtub3duLWxpbnV4LWdudScsXG4gICAgJ2FhcmNoNjQtbGludXgtYW5kcm9pZCcsXG4gICAgJ2FybXY3LWxpbnV4LWFuZHJvaWRlYWJpJyxcbiAgXSlcblxuICAvLyBDaGVjayBpZiBhbnkgTGludXggdGFyZ2V0cyBhcmUgZW5hYmxlZFxuICBjb25zdCBoYXNMaW51eFRhcmdldHMgPSBlbmFibGVkVGFyZ2V0cy5zb21lKCh0YXJnZXQpID0+XG4gICAgbGludXhUYXJnZXRzLmhhcyh0YXJnZXQpLFxuICApXG5cbiAgLy8gRmlsdGVyIHRoZSBtYXRyaXggY29uZmlndXJhdGlvbnMgaW4gdGhlIGJ1aWxkIGpvYlxuICBpZiAoeWFtbD8uam9icz8uYnVpbGQ/LnN0cmF0ZWd5Py5tYXRyaXg/LnNldHRpbmdzKSB7XG4gICAgeWFtbC5qb2JzLmJ1aWxkLnN0cmF0ZWd5Lm1hdHJpeC5zZXR0aW5ncyA9XG4gICAgICB5YW1sLmpvYnMuYnVpbGQuc3RyYXRlZ3kubWF0cml4LnNldHRpbmdzLmZpbHRlcigoc2V0dGluZzogYW55KSA9PiB7XG4gICAgICAgIGlmIChzZXR0aW5nLnRhcmdldCkge1xuICAgICAgICAgIHJldHVybiBlbmFibGVkVGFyZ2V0cy5pbmNsdWRlcyhzZXR0aW5nLnRhcmdldClcbiAgICAgICAgfVxuICAgICAgICByZXR1cm4gdHJ1ZVxuICAgICAgfSlcbiAgfVxuXG4gIGNvbnN0IGpvYnNUb1JlbW92ZTogc3RyaW5nW10gPSBbXVxuXG4gIGlmIChlbmFibGVkVGFyZ2V0cy5ldmVyeSgodGFyZ2V0KSA9PiAhbWFjT1NBbmRXaW5kb3dzVGFyZ2V0cy5oYXModGFyZ2V0KSkpIHtcbiAgICBqb2JzVG9SZW1vdmUucHVzaCgndGVzdC1tYWNPUy13aW5kb3dzLWJpbmRpbmcnKVxuICB9IGVsc2Uge1xuICAgIC8vIEZpbHRlciB0aGUgbWF0cml4IGNvbmZpZ3VyYXRpb25zIGluIHRoZSB0ZXN0LW1hY09TLXdpbmRvd3MtYmluZGluZyBqb2JcbiAgICBpZiAoXG4gICAgICB5YW1sPy5qb2JzPy5bJ3Rlc3QtbWFjT1Mtd2luZG93cy1iaW5kaW5nJ10/LnN0cmF0ZWd5Py5tYXRyaXg/LnNldHRpbmdzXG4gICAgKSB7XG4gICAgICB5YW1sLmpvYnNbJ3Rlc3QtbWFjT1Mtd2luZG93cy1iaW5kaW5nJ10uc3RyYXRlZ3kubWF0cml4LnNldHRpbmdzID1cbiAgICAgICAgeWFtbC5qb2JzWyd0ZXN0LW1hY09TLXdpbmRvd3MtYmluZGluZyddLnN0cmF0ZWd5Lm1hdHJpeC5zZXR0aW5ncy5maWx0ZXIoXG4gICAgICAgICAgKHNldHRpbmc6IGFueSkgPT4ge1xuICAgICAgICAgICAgaWYgKHNldHRpbmcudGFyZ2V0KSB7XG4gICAgICAgICAgICAgIHJldHVybiBlbmFibGVkVGFyZ2V0cy5pbmNsdWRlcyhzZXR0aW5nLnRhcmdldClcbiAgICAgICAgICAgIH1cbiAgICAgICAgICAgIHJldHVybiB0cnVlXG4gICAgICAgICAgfSxcbiAgICAgICAgKVxuICAgIH1cbiAgfVxuXG4gIC8vIElmIG5vIExpbnV4IHRhcmdldHMgYXJlIGVuYWJsZWQsIHJlbW92ZSBMaW51eC1zcGVjaWZpYyBqb2JzXG4gIGlmICghaGFzTGludXhUYXJnZXRzKSB7XG4gICAgLy8gUmVtb3ZlIHRlc3QtbGludXgtYmluZGluZyBqb2JcbiAgICBpZiAoeWFtbD8uam9icz8uWyd0ZXN0LWxpbnV4LWJpbmRpbmcnXSkge1xuICAgICAgam9ic1RvUmVtb3ZlLnB1c2goJ3Rlc3QtbGludXgtYmluZGluZycpXG4gICAgfVxuICB9IGVsc2Uge1xuICAgIC8vIEZpbHRlciB0aGUgbWF0cml4IGNvbmZpZ3VyYXRpb25zIGluIHRoZSB0ZXN0LWxpbnV4LXg2NC1nbnUtYmluZGluZyBqb2JcbiAgICBpZiAoeWFtbD8uam9icz8uWyd0ZXN0LWxpbnV4LWJpbmRpbmcnXT8uc3RyYXRlZ3k/Lm1hdHJpeD8udGFyZ2V0KSB7XG4gICAgICB5YW1sLmpvYnNbJ3Rlc3QtbGludXgtYmluZGluZyddLnN0cmF0ZWd5Lm1hdHJpeC50YXJnZXQgPSB5YW1sLmpvYnNbXG4gICAgICAgICd0ZXN0LWxpbnV4LWJpbmRpbmcnXG4gICAgICBdLnN0cmF0ZWd5Lm1hdHJpeC50YXJnZXQuZmlsdGVyKCh0YXJnZXQ6IHN0cmluZykgPT4ge1xuICAgICAgICBpZiAodGFyZ2V0KSB7XG4gICAgICAgICAgcmV0dXJuIGVuYWJsZWRUYXJnZXRzLmluY2x1ZGVzKHRhcmdldClcbiAgICAgICAgfVxuICAgICAgICByZXR1cm4gdHJ1ZVxuICAgICAgfSlcbiAgICB9XG4gIH1cblxuICBpZiAoIWVuYWJsZWRUYXJnZXRzLmluY2x1ZGVzKCd3YXNtMzItd2FzaXAxLXRocmVhZHMnKSkge1xuICAgIGpvYnNUb1JlbW92ZS5wdXNoKCd0ZXN0LXdhc2knKVxuICB9XG5cbiAgaWYgKCFlbmFibGVkVGFyZ2V0cy5pbmNsdWRlcygneDg2XzY0LXVua25vd24tZnJlZWJzZCcpKSB7XG4gICAgam9ic1RvUmVtb3ZlLnB1c2goJ2J1aWxkLWZyZWVic2QnKVxuICB9XG5cbiAgLy8gRmlsdGVyIG90aGVyIHRlc3Qgam9icyBiYXNlZCBvbiB0YXJnZXRcbiAgZm9yIChjb25zdCBbam9iTmFtZSwgam9iQ29uZmlnXSBvZiBPYmplY3QuZW50cmllcyh5YW1sLmpvYnMgfHwge30pKSB7XG4gICAgaWYgKFxuICAgICAgam9iTmFtZS5zdGFydHNXaXRoKCd0ZXN0LScpICYmXG4gICAgICBqb2JOYW1lICE9PSAndGVzdC1tYWNPUy13aW5kb3dzLWJpbmRpbmcnICYmXG4gICAgICBqb2JOYW1lICE9PSAndGVzdC1saW51eC14NjQtZ251LWJpbmRpbmcnXG4gICAgKSB7XG4gICAgICAvLyBFeHRyYWN0IHRhcmdldCBmcm9tIGpvYiBuYW1lIG9yIGNvbmZpZ1xuICAgICAgY29uc3Qgam9iID0gam9iQ29uZmlnIGFzIGFueVxuICAgICAgaWYgKGpvYi5zdHJhdGVneT8ubWF0cml4Py5zZXR0aW5ncz8uWzBdPy50YXJnZXQpIHtcbiAgICAgICAgY29uc3QgdGFyZ2V0ID0gam9iLnN0cmF0ZWd5Lm1hdHJpeC5zZXR0aW5nc1swXS50YXJnZXRcbiAgICAgICAgaWYgKCFlbmFibGVkVGFyZ2V0cy5pbmNsdWRlcyh0YXJnZXQpKSB7XG4gICAgICAgICAgam9ic1RvUmVtb3ZlLnB1c2goam9iTmFtZSlcbiAgICAgICAgfVxuICAgICAgfVxuICAgIH1cbiAgfVxuXG4gIC8vIFJlbW92ZSBqb2JzIGZvciBkaXNhYmxlZCB0YXJnZXRzXG4gIGZvciAoY29uc3Qgam9iTmFtZSBvZiBqb2JzVG9SZW1vdmUpIHtcbiAgICBkZWxldGUgeWFtbC5qb2JzW2pvYk5hbWVdXG4gIH1cblxuICBpZiAoQXJyYXkuaXNBcnJheSh5YW1sLmpvYnM/LnB1Ymxpc2g/Lm5lZWRzKSkge1xuICAgIHlhbWwuam9icy5wdWJsaXNoLm5lZWRzID0geWFtbC5qb2JzLnB1Ymxpc2gubmVlZHMuZmlsdGVyKFxuICAgICAgKG5lZWQ6IHN0cmluZykgPT4gIWpvYnNUb1JlbW92ZS5pbmNsdWRlcyhuZWVkKSxcbiAgICApXG4gIH1cblxuICAvLyBXcml0ZSBiYWNrIHRoZSBmaWx0ZXJlZCBZQU1MXG4gIGNvbnN0IHVwZGF0ZWRZYW1sID0geWFtbER1bXAoeWFtbCwge1xuICAgIGxpbmVXaWR0aDogLTEsXG4gICAgbm9SZWZzOiB0cnVlLFxuICAgIHNvcnRLZXlzOiBmYWxzZSxcbiAgfSlcbiAgYXdhaXQgZnMud3JpdGVGaWxlKGZpbGVQYXRoLCB1cGRhdGVkWWFtbClcbn1cblxuZnVuY3Rpb24gcHJvY2Vzc09wdGlvbnMob3B0aW9uczogUmF3TmV3T3B0aW9ucykge1xuICBkZWJ1ZygnUHJvY2Vzc2luZyBvcHRpb25zLi4uJylcbiAgaWYgKCFvcHRpb25zLnBhdGgpIHtcbiAgICB0aHJvdyBuZXcgRXJyb3IoJ1BsZWFzZSBwcm92aWRlIHRoZSBwYXRoIGFzIHRoZSBhcmd1bWVudCcpXG4gIH1cbiAgb3B0aW9ucy5wYXRoID0gcGF0aC5yZXNvbHZlKHByb2Nlc3MuY3dkKCksIG9wdGlvbnMucGF0aClcbiAgZGVidWcoYFJlc29sdmVkIHRhcmdldCBwYXRoIHRvOiAke29wdGlvbnMucGF0aH1gKVxuXG4gIGlmICghb3B0aW9ucy5uYW1lKSB7XG4gICAgb3B0aW9ucy5uYW1lID0gcGF0aC5wYXJzZShvcHRpb25zLnBhdGgpLmJhc2VcbiAgICBkZWJ1ZyhgTm8gcHJvamVjdCBuYW1lIHByb3ZpZGVkLCBmaXggaXQgdG8gZGlyIG5hbWU6ICR7b3B0aW9ucy5uYW1lfWApXG4gIH1cblxuICBpZiAoIW9wdGlvbnMudGFyZ2V0cz8ubGVuZ3RoKSB7XG4gICAgaWYgKG9wdGlvbnMuZW5hYmxlQWxsVGFyZ2V0cykge1xuICAgICAgb3B0aW9ucy50YXJnZXRzID0gQVZBSUxBQkxFX1RBUkdFVFMuY29uY2F0KClcbiAgICAgIGRlYnVnKCdFbmFibGUgYWxsIHRhcmdldHMnKVxuICAgIH0gZWxzZSBpZiAob3B0aW9ucy5lbmFibGVEZWZhdWx0VGFyZ2V0cykge1xuICAgICAgb3B0aW9ucy50YXJnZXRzID0gREVGQVVMVF9UQVJHRVRTLmNvbmNhdCgpXG4gICAgICBkZWJ1ZygnRW5hYmxlIGRlZmF1bHQgdGFyZ2V0cycpXG4gICAgfSBlbHNlIHtcbiAgICAgIHRocm93IG5ldyBFcnJvcignQXQgbGVhc3Qgb25lIHRhcmdldCBtdXN0IGJlIGVuYWJsZWQnKVxuICAgIH1cbiAgfVxuICBpZiAoXG4gICAgb3B0aW9ucy50YXJnZXRzLnNvbWUoKHRhcmdldCkgPT4gdGFyZ2V0ID09PSAnd2FzbTMyLXdhc2ktcHJldmlldzEtdGhyZWFkcycpXG4gICkge1xuICAgIGNvbnN0IG91dCA9IGV4ZWNTeW5jKGBydXN0dXAgdGFyZ2V0IGxpc3RgLCB7XG4gICAgICBlbmNvZGluZzogJ3V0ZjgnLFxuICAgIH0pXG4gICAgaWYgKG91dC5pbmNsdWRlcygnd2FzbTMyLXdhc2lwMS10aHJlYWRzJykpIHtcbiAgICAgIG9wdGlvbnMudGFyZ2V0cyA9IG9wdGlvbnMudGFyZ2V0cy5tYXAoKHRhcmdldCkgPT5cbiAgICAgICAgdGFyZ2V0ID09PSAnd2FzbTMyLXdhc2ktcHJldmlldzEtdGhyZWFkcydcbiAgICAgICAgICA/ICd3YXNtMzItd2FzaXAxLXRocmVhZHMnXG4gICAgICAgICAgOiB0YXJnZXQsXG4gICAgICApXG4gICAgfVxuICB9XG5cbiAgcmV0dXJuIGFwcGx5RGVmYXVsdE5ld09wdGlvbnMob3B0aW9ucykgYXMgTmV3T3B0aW9uc1xufVxuXG5leHBvcnQgYXN5bmMgZnVuY3Rpb24gbmV3UHJvamVjdCh1c2VyT3B0aW9uczogUmF3TmV3T3B0aW9ucykge1xuICBkZWJ1ZygnV2lsbCBjcmVhdGUgbmFwaS1ycyBwcm9qZWN0IHdpdGggZ2l2ZW4gb3B0aW9uczonKVxuICBkZWJ1Zyh1c2VyT3B0aW9ucylcblxuICBjb25zdCBvcHRpb25zID0gcHJvY2Vzc09wdGlvbnModXNlck9wdGlvbnMpXG5cbiAgZGVidWcoJ1RhcmdldHMgdG8gYmUgZW5hYmxlZDonKVxuICBkZWJ1ZyhvcHRpb25zLnRhcmdldHMpXG5cbiAgLy8gQ2hlY2sgaWYgZ2l0IGlzIGF2YWlsYWJsZVxuICBpZiAoIShhd2FpdCBjaGVja0dpdENvbW1hbmQoKSkpIHtcbiAgICB0aHJvdyBuZXcgRXJyb3IoXG4gICAgICAnR2l0IGlzIG5vdCBpbnN0YWxsZWQgb3Igbm90IGF2YWlsYWJsZSBpbiBQQVRILiBQbGVhc2UgaW5zdGFsbCBHaXQgdG8gY29udGludWUuJyxcbiAgICApXG4gIH1cblxuICBjb25zdCBwYWNrYWdlTWFuYWdlciA9IG9wdGlvbnMucGFja2FnZU1hbmFnZXIgYXMgU3VwcG9ydGVkUGFja2FnZU1hbmFnZXJcblxuICAvLyBFbnN1cmUgdGFyZ2V0IGRpcmVjdG9yeSBleGlzdHMgYW5kIGlzIGVtcHR5XG4gIGF3YWl0IGVuc3VyZVBhdGgob3B0aW9ucy5wYXRoLCBvcHRpb25zLmRyeVJ1bilcblxuICBpZiAoIW9wdGlvbnMuZHJ5UnVuKSB7XG4gICAgdHJ5IHtcbiAgICAgIC8vIERvd25sb2FkIG9yIHVwZGF0ZSB0ZW1wbGF0ZVxuICAgICAgY29uc3QgY2FjaGVEaXIgPSBhd2FpdCBlbnN1cmVDYWNoZURpcihwYWNrYWdlTWFuYWdlcilcbiAgICAgIGF3YWl0IGRvd25sb2FkVGVtcGxhdGUocGFja2FnZU1hbmFnZXIsIGNhY2hlRGlyKVxuXG4gICAgICAvLyBDb3B5IHRlbXBsYXRlIGZpbGVzIHRvIHRhcmdldCBkaXJlY3RvcnlcbiAgICAgIGNvbnN0IHRlbXBsYXRlUGF0aCA9IHBhdGguam9pbihjYWNoZURpciwgJ3JlcG8nKVxuICAgICAgYXdhaXQgY29weURpcmVjdG9yeShcbiAgICAgICAgdGVtcGxhdGVQYXRoLFxuICAgICAgICBvcHRpb25zLnBhdGgsXG4gICAgICAgIG9wdGlvbnMudGFyZ2V0cy5pbmNsdWRlcygnd2FzbTMyLXdhc2lwMS10aHJlYWRzJyksXG4gICAgICApXG5cbiAgICAgIC8vIFJlbmFtZSBwcm9qZWN0IHVzaW5nIHRoZSByZW5hbWUgQVBJXG4gICAgICBhd2FpdCByZW5hbWVQcm9qZWN0KHtcbiAgICAgICAgY3dkOiBvcHRpb25zLnBhdGgsXG4gICAgICAgIG5hbWU6IG9wdGlvbnMubmFtZSxcbiAgICAgICAgYmluYXJ5TmFtZTogZ2V0QmluYXJ5TmFtZShvcHRpb25zLm5hbWUpLFxuICAgICAgfSlcblxuICAgICAgLy8gRmlsdGVyIHRhcmdldHMgaW4gcGFja2FnZS5qc29uXG4gICAgICBjb25zdCBwYWNrYWdlSnNvblBhdGggPSBwYXRoLmpvaW4ob3B0aW9ucy5wYXRoLCAncGFja2FnZS5qc29uJylcbiAgICAgIGlmIChleGlzdHNTeW5jKHBhY2thZ2VKc29uUGF0aCkpIHtcbiAgICAgICAgYXdhaXQgZmlsdGVyVGFyZ2V0c0luUGFja2FnZUpzb24ocGFja2FnZUpzb25QYXRoLCBvcHRpb25zLnRhcmdldHMpXG4gICAgICB9XG5cbiAgICAgIC8vIEZpbHRlciB0YXJnZXRzIGluIEdpdEh1YiBBY3Rpb25zIENJXG4gICAgICBjb25zdCBjaVBhdGggPSBwYXRoLmpvaW4ob3B0aW9ucy5wYXRoLCAnLmdpdGh1YicsICd3b3JrZmxvd3MnLCAnQ0kueW1sJylcbiAgICAgIGlmIChleGlzdHNTeW5jKGNpUGF0aCkgJiYgb3B0aW9ucy5lbmFibGVHaXRodWJBY3Rpb25zKSB7XG4gICAgICAgIGF3YWl0IGZpbHRlclRhcmdldHNJbkdpdGh1YkFjdGlvbnMoY2lQYXRoLCBvcHRpb25zLnRhcmdldHMpXG4gICAgICB9IGVsc2UgaWYgKFxuICAgICAgICAhb3B0aW9ucy5lbmFibGVHaXRodWJBY3Rpb25zICYmXG4gICAgICAgIGV4aXN0c1N5bmMocGF0aC5qb2luKG9wdGlvbnMucGF0aCwgJy5naXRodWInKSlcbiAgICAgICkge1xuICAgICAgICAvLyBSZW1vdmUgLmdpdGh1YiBkaXJlY3RvcnkgaWYgR2l0SHViIEFjdGlvbnMgaXMgbm90IGVuYWJsZWRcbiAgICAgICAgYXdhaXQgZnMucm0ocGF0aC5qb2luKG9wdGlvbnMucGF0aCwgJy5naXRodWInKSwge1xuICAgICAgICAgIHJlY3Vyc2l2ZTogdHJ1ZSxcbiAgICAgICAgICBmb3JjZTogdHJ1ZSxcbiAgICAgICAgfSlcbiAgICAgIH1cblxuICAgICAgLy8gVXBkYXRlIHBhY2thZ2UuanNvbiB3aXRoIGFkZGl0aW9uYWwgY29uZmlndXJhdGlvbnNcbiAgICAgIGNvbnN0IHBrZ0pzb25Db250ZW50ID0gYXdhaXQgZnMucmVhZEZpbGUocGFja2FnZUpzb25QYXRoLCAndXRmLTgnKVxuICAgICAgY29uc3QgcGtnSnNvbiA9IEpTT04ucGFyc2UocGtnSnNvbkNvbnRlbnQpXG5cbiAgICAgIC8vIFVwZGF0ZSBlbmdpbmUgcmVxdWlyZW1lbnRcbiAgICAgIGlmICghcGtnSnNvbi5lbmdpbmVzKSB7XG4gICAgICAgIHBrZ0pzb24uZW5naW5lcyA9IHt9XG4gICAgICB9XG4gICAgICBwa2dKc29uLmVuZ2luZXMubm9kZSA9IG5hcGlFbmdpbmVSZXF1aXJlbWVudChvcHRpb25zLm1pbk5vZGVBcGlWZXJzaW9uKVxuXG4gICAgICAvLyBVcGRhdGUgbGljZW5zZSBpZiBkaWZmZXJlbnQgZnJvbSB0ZW1wbGF0ZVxuICAgICAgaWYgKG9wdGlvbnMubGljZW5zZSAmJiBwa2dKc29uLmxpY2Vuc2UgIT09IG9wdGlvbnMubGljZW5zZSkge1xuICAgICAgICBwa2dKc29uLmxpY2Vuc2UgPSBvcHRpb25zLmxpY2Vuc2VcbiAgICAgIH1cblxuICAgICAgLy8gVXBkYXRlIHRlc3QgZnJhbWV3b3JrIGlmIG5lZWRlZFxuICAgICAgaWYgKG9wdGlvbnMudGVzdEZyYW1ld29yayAhPT0gJ2F2YScpIHtcbiAgICAgICAgLy8gVGhpcyB3b3VsZCByZXF1aXJlIG1vcmUgY29tcGxleCBsb2dpYyB0byB1cGRhdGUgdGVzdCBzY3JpcHRzIGFuZCBkZXBlbmRlbmNpZXNcbiAgICAgICAgZGVidWcoXG4gICAgICAgICAgYFRlc3QgZnJhbWV3b3JrICR7b3B0aW9ucy50ZXN0RnJhbWV3b3JrfSByZXF1ZXN0ZWQgYnV0IG5vdCB5ZXQgaW1wbGVtZW50ZWRgLFxuICAgICAgICApXG4gICAgICB9XG5cbiAgICAgIGF3YWl0IGZzLndyaXRlRmlsZShcbiAgICAgICAgcGFja2FnZUpzb25QYXRoLFxuICAgICAgICBKU09OLnN0cmluZ2lmeShwa2dKc29uLCBudWxsLCAyKSArICdcXG4nLFxuICAgICAgKVxuICAgIH0gY2F0Y2ggKGVycm9yKSB7XG4gICAgICB0aHJvdyBuZXcgRXJyb3IoYEZhaWxlZCB0byBjcmVhdGUgcHJvamVjdDogJHtlcnJvcn1gKVxuICAgIH1cbiAgfVxuXG4gIGRlYnVnKGBQcm9qZWN0IGNyZWF0ZWQgYXQ6ICR7b3B0aW9ucy5wYXRofWApXG59XG5cbmFzeW5jIGZ1bmN0aW9uIGVuc3VyZVBhdGgocGF0aDogc3RyaW5nLCBkcnlSdW4gPSBmYWxzZSkge1xuICBjb25zdCBzdGF0ID0gYXdhaXQgc3RhdEFzeW5jKHBhdGgsIHt9KS5jYXRjaCgoKSA9PiB1bmRlZmluZWQpXG5cbiAgLy8gZmlsZSBkZXNjcmlwdG9yIGV4aXN0c1xuICBpZiAoc3RhdCkge1xuICAgIGlmIChzdGF0LmlzRmlsZSgpKSB7XG4gICAgICB0aHJvdyBuZXcgRXJyb3IoXG4gICAgICAgIGBQYXRoICR7cGF0aH0gZm9yIGNyZWF0aW5nIG5ldyBuYXBpLXJzIHByb2plY3QgYWxyZWFkeSBleGlzdHMgYW5kIGl0J3Mgbm90IGEgZGlyZWN0b3J5LmAsXG4gICAgICApXG4gICAgfSBlbHNlIGlmIChzdGF0LmlzRGlyZWN0b3J5KCkpIHtcbiAgICAgIGNvbnN0IGZpbGVzID0gYXdhaXQgcmVhZGRpckFzeW5jKHBhdGgpXG4gICAgICBpZiAoZmlsZXMubGVuZ3RoKSB7XG4gICAgICAgIHRocm93IG5ldyBFcnJvcihcbiAgICAgICAgICBgUGF0aCAke3BhdGh9IGZvciBjcmVhdGluZyBuZXcgbmFwaS1ycyBwcm9qZWN0IGFscmVhZHkgZXhpc3RzIGFuZCBpdCdzIG5vdCBlbXB0eS5gLFxuICAgICAgICApXG4gICAgICB9XG4gICAgfVxuICB9XG5cbiAgaWYgKCFkcnlSdW4pIHtcbiAgICB0cnkge1xuICAgICAgZGVidWcoYFRyeSB0byBjcmVhdGUgdGFyZ2V0IGRpcmVjdG9yeTogJHtwYXRofWApXG4gICAgICBpZiAoIWRyeVJ1bikge1xuICAgICAgICBhd2FpdCBta2RpckFzeW5jKHBhdGgsIHsgcmVjdXJzaXZlOiB0cnVlIH0pXG4gICAgICB9XG4gICAgfSBjYXRjaCAoZSkge1xuICAgICAgdGhyb3cgbmV3IEVycm9yKGBGYWlsZWQgdG8gY3JlYXRlIHRhcmdldCBkaXJlY3Rvcnk6ICR7cGF0aH1gLCB7XG4gICAgICAgIGNhdXNlOiBlLFxuICAgICAgfSlcbiAgICB9XG4gIH1cbn1cblxuZnVuY3Rpb24gZ2V0QmluYXJ5TmFtZShuYW1lOiBzdHJpbmcpOiBzdHJpbmcge1xuICByZXR1cm4gbmFtZS5zcGxpdCgnLycpLnBvcCgpIVxufVxuXG5leHBvcnQgdHlwZSB7IE5ld09wdGlvbnMgfVxuIiwiLy8gVGhpcyBmaWxlIGlzIGdlbmVyYXRlZCBieSBjb2RlZ2VuL2luZGV4LnRzXG4vLyBEbyBub3QgZWRpdCB0aGlzIGZpbGUgbWFudWFsbHlcbmltcG9ydCB7IENvbW1hbmQsIE9wdGlvbiB9IGZyb20gJ2NsaXBhbmlvbidcblxuZXhwb3J0IGFic3RyYWN0IGNsYXNzIEJhc2VQcmVQdWJsaXNoQ29tbWFuZCBleHRlbmRzIENvbW1hbmQge1xuICBzdGF0aWMgcGF0aHMgPSBbWydwcmUtcHVibGlzaCddLCBbJ3ByZXB1Ymxpc2gnXV1cblxuICBzdGF0aWMgdXNhZ2UgPSBDb21tYW5kLlVzYWdlKHtcbiAgICBkZXNjcmlwdGlvbjpcbiAgICAgICdVcGRhdGUgcGFja2FnZS5qc29uIGFuZCBjb3B5IGFkZG9ucyBpbnRvIHBlciBwbGF0Zm9ybSBwYWNrYWdlcycsXG4gIH0pXG5cbiAgY3dkID0gT3B0aW9uLlN0cmluZygnLS1jd2QnLCBwcm9jZXNzLmN3ZCgpLCB7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnVGhlIHdvcmtpbmcgZGlyZWN0b3J5IG9mIHdoZXJlIG5hcGkgY29tbWFuZCB3aWxsIGJlIGV4ZWN1dGVkIGluLCBhbGwgb3RoZXIgcGF0aHMgb3B0aW9ucyBhcmUgcmVsYXRpdmUgdG8gdGhpcyBwYXRoJyxcbiAgfSlcblxuICBjb25maWdQYXRoPzogc3RyaW5nID0gT3B0aW9uLlN0cmluZygnLS1jb25maWctcGF0aCwtYycsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1BhdGggdG8gYG5hcGlgIGNvbmZpZyBqc29uIGZpbGUnLFxuICB9KVxuXG4gIHBhY2thZ2VKc29uUGF0aCA9IE9wdGlvbi5TdHJpbmcoJy0tcGFja2FnZS1qc29uLXBhdGgnLCAncGFja2FnZS5qc29uJywge1xuICAgIGRlc2NyaXB0aW9uOiAnUGF0aCB0byBgcGFja2FnZS5qc29uYCcsXG4gIH0pXG5cbiAgbnBtRGlyID0gT3B0aW9uLlN0cmluZygnLS1ucG0tZGlyLC1wJywgJ25wbScsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1BhdGggdG8gdGhlIGZvbGRlciB3aGVyZSB0aGUgbnBtIHBhY2thZ2VzIHB1dCcsXG4gIH0pXG5cbiAgdGFnU3R5bGUgPSBPcHRpb24uU3RyaW5nKCctLXRhZy1zdHlsZSwtLXRhZ3N0eWxlLC10JywgJ2xlcm5hJywge1xuICAgIGRlc2NyaXB0aW9uOiAnZ2l0IHRhZyBzdHlsZSwgYG5wbWAgb3IgYGxlcm5hYCcsXG4gIH0pXG5cbiAgZ2hSZWxlYXNlID0gT3B0aW9uLkJvb2xlYW4oJy0tZ2gtcmVsZWFzZScsIHRydWUsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1doZXRoZXIgY3JlYXRlIEdpdEh1YiByZWxlYXNlJyxcbiAgfSlcblxuICBnaFJlbGVhc2VOYW1lPzogc3RyaW5nID0gT3B0aW9uLlN0cmluZygnLS1naC1yZWxlYXNlLW5hbWUnLCB7XG4gICAgZGVzY3JpcHRpb246ICdHaXRIdWIgcmVsZWFzZSBuYW1lJyxcbiAgfSlcblxuICBnaFJlbGVhc2VJZD86IHN0cmluZyA9IE9wdGlvbi5TdHJpbmcoJy0tZ2gtcmVsZWFzZS1pZCcsIHtcbiAgICBkZXNjcmlwdGlvbjogJ0V4aXN0aW5nIEdpdEh1YiByZWxlYXNlIGlkJyxcbiAgfSlcblxuICBza2lwT3B0aW9uYWxQdWJsaXNoID0gT3B0aW9uLkJvb2xlYW4oJy0tc2tpcC1vcHRpb25hbC1wdWJsaXNoJywgZmFsc2UsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1doZXRoZXIgc2tpcCBvcHRpb25hbERlcGVuZGVuY2llcyBwYWNrYWdlcyBwdWJsaXNoJyxcbiAgfSlcblxuICBkcnlSdW4gPSBPcHRpb24uQm9vbGVhbignLS1kcnktcnVuJywgZmFsc2UsIHtcbiAgICBkZXNjcmlwdGlvbjogJ0RyeSBydW4gd2l0aG91dCB0b3VjaGluZyBmaWxlIHN5c3RlbScsXG4gIH0pXG5cbiAgZ2V0T3B0aW9ucygpIHtcbiAgICByZXR1cm4ge1xuICAgICAgY3dkOiB0aGlzLmN3ZCxcbiAgICAgIGNvbmZpZ1BhdGg6IHRoaXMuY29uZmlnUGF0aCxcbiAgICAgIHBhY2thZ2VKc29uUGF0aDogdGhpcy5wYWNrYWdlSnNvblBhdGgsXG4gICAgICBucG1EaXI6IHRoaXMubnBtRGlyLFxuICAgICAgdGFnU3R5bGU6IHRoaXMudGFnU3R5bGUsXG4gICAgICBnaFJlbGVhc2U6IHRoaXMuZ2hSZWxlYXNlLFxuICAgICAgZ2hSZWxlYXNlTmFtZTogdGhpcy5naFJlbGVhc2VOYW1lLFxuICAgICAgZ2hSZWxlYXNlSWQ6IHRoaXMuZ2hSZWxlYXNlSWQsXG4gICAgICBza2lwT3B0aW9uYWxQdWJsaXNoOiB0aGlzLnNraXBPcHRpb25hbFB1Ymxpc2gsXG4gICAgICBkcnlSdW46IHRoaXMuZHJ5UnVuLFxuICAgIH1cbiAgfVxufVxuXG4vKipcbiAqIFVwZGF0ZSBwYWNrYWdlLmpzb24gYW5kIGNvcHkgYWRkb25zIGludG8gcGVyIHBsYXRmb3JtIHBhY2thZ2VzXG4gKi9cbmV4cG9ydCBpbnRlcmZhY2UgUHJlUHVibGlzaE9wdGlvbnMge1xuICAvKipcbiAgICogVGhlIHdvcmtpbmcgZGlyZWN0b3J5IG9mIHdoZXJlIG5hcGkgY29tbWFuZCB3aWxsIGJlIGV4ZWN1dGVkIGluLCBhbGwgb3RoZXIgcGF0aHMgb3B0aW9ucyBhcmUgcmVsYXRpdmUgdG8gdGhpcyBwYXRoXG4gICAqXG4gICAqIEBkZWZhdWx0IHByb2Nlc3MuY3dkKClcbiAgICovXG4gIGN3ZD86IHN0cmluZ1xuICAvKipcbiAgICogUGF0aCB0byBgbmFwaWAgY29uZmlnIGpzb24gZmlsZVxuICAgKi9cbiAgY29uZmlnUGF0aD86IHN0cmluZ1xuICAvKipcbiAgICogUGF0aCB0byBgcGFja2FnZS5qc29uYFxuICAgKlxuICAgKiBAZGVmYXVsdCAncGFja2FnZS5qc29uJ1xuICAgKi9cbiAgcGFja2FnZUpzb25QYXRoPzogc3RyaW5nXG4gIC8qKlxuICAgKiBQYXRoIHRvIHRoZSBmb2xkZXIgd2hlcmUgdGhlIG5wbSBwYWNrYWdlcyBwdXRcbiAgICpcbiAgICogQGRlZmF1bHQgJ25wbSdcbiAgICovXG4gIG5wbURpcj86IHN0cmluZ1xuICAvKipcbiAgICogZ2l0IHRhZyBzdHlsZSwgYG5wbWAgb3IgYGxlcm5hYFxuICAgKlxuICAgKiBAZGVmYXVsdCAnbGVybmEnXG4gICAqL1xuICB0YWdTdHlsZT86ICducG0nIHwgJ2xlcm5hJ1xuICAvKipcbiAgICogV2hldGhlciBjcmVhdGUgR2l0SHViIHJlbGVhc2VcbiAgICpcbiAgICogQGRlZmF1bHQgdHJ1ZVxuICAgKi9cbiAgZ2hSZWxlYXNlPzogYm9vbGVhblxuICAvKipcbiAgICogR2l0SHViIHJlbGVhc2UgbmFtZVxuICAgKi9cbiAgZ2hSZWxlYXNlTmFtZT86IHN0cmluZ1xuICAvKipcbiAgICogRXhpc3RpbmcgR2l0SHViIHJlbGVhc2UgaWRcbiAgICovXG4gIGdoUmVsZWFzZUlkPzogc3RyaW5nXG4gIC8qKlxuICAgKiBXaGV0aGVyIHNraXAgb3B0aW9uYWxEZXBlbmRlbmNpZXMgcGFja2FnZXMgcHVibGlzaFxuICAgKlxuICAgKiBAZGVmYXVsdCBmYWxzZVxuICAgKi9cbiAgc2tpcE9wdGlvbmFsUHVibGlzaD86IGJvb2xlYW5cbiAgLyoqXG4gICAqIERyeSBydW4gd2l0aG91dCB0b3VjaGluZyBmaWxlIHN5c3RlbVxuICAgKlxuICAgKiBAZGVmYXVsdCBmYWxzZVxuICAgKi9cbiAgZHJ5UnVuPzogYm9vbGVhblxufVxuXG5leHBvcnQgZnVuY3Rpb24gYXBwbHlEZWZhdWx0UHJlUHVibGlzaE9wdGlvbnMob3B0aW9uczogUHJlUHVibGlzaE9wdGlvbnMpIHtcbiAgcmV0dXJuIHtcbiAgICBjd2Q6IHByb2Nlc3MuY3dkKCksXG4gICAgcGFja2FnZUpzb25QYXRoOiAncGFja2FnZS5qc29uJyxcbiAgICBucG1EaXI6ICducG0nLFxuICAgIHRhZ1N0eWxlOiAnbGVybmEnLFxuICAgIGdoUmVsZWFzZTogdHJ1ZSxcbiAgICBza2lwT3B0aW9uYWxQdWJsaXNoOiBmYWxzZSxcbiAgICBkcnlSdW46IGZhbHNlLFxuICAgIC4uLm9wdGlvbnMsXG4gIH1cbn1cbiIsIi8vIFRoaXMgZmlsZSBpcyBnZW5lcmF0ZWQgYnkgY29kZWdlbi9pbmRleC50c1xuLy8gRG8gbm90IGVkaXQgdGhpcyBmaWxlIG1hbnVhbGx5XG5pbXBvcnQgeyBDb21tYW5kLCBPcHRpb24gfSBmcm9tICdjbGlwYW5pb24nXG5cbmV4cG9ydCBhYnN0cmFjdCBjbGFzcyBCYXNlVmVyc2lvbkNvbW1hbmQgZXh0ZW5kcyBDb21tYW5kIHtcbiAgc3RhdGljIHBhdGhzID0gW1sndmVyc2lvbiddXVxuXG4gIHN0YXRpYyB1c2FnZSA9IENvbW1hbmQuVXNhZ2Uoe1xuICAgIGRlc2NyaXB0aW9uOiAnVXBkYXRlIHZlcnNpb24gaW4gY3JlYXRlZCBucG0gcGFja2FnZXMnLFxuICB9KVxuXG4gIGN3ZCA9IE9wdGlvbi5TdHJpbmcoJy0tY3dkJywgcHJvY2Vzcy5jd2QoKSwge1xuICAgIGRlc2NyaXB0aW9uOlxuICAgICAgJ1RoZSB3b3JraW5nIGRpcmVjdG9yeSBvZiB3aGVyZSBuYXBpIGNvbW1hbmQgd2lsbCBiZSBleGVjdXRlZCBpbiwgYWxsIG90aGVyIHBhdGhzIG9wdGlvbnMgYXJlIHJlbGF0aXZlIHRvIHRoaXMgcGF0aCcsXG4gIH0pXG5cbiAgY29uZmlnUGF0aD86IHN0cmluZyA9IE9wdGlvbi5TdHJpbmcoJy0tY29uZmlnLXBhdGgsLWMnLCB7XG4gICAgZGVzY3JpcHRpb246ICdQYXRoIHRvIGBuYXBpYCBjb25maWcganNvbiBmaWxlJyxcbiAgfSlcblxuICBwYWNrYWdlSnNvblBhdGggPSBPcHRpb24uU3RyaW5nKCctLXBhY2thZ2UtanNvbi1wYXRoJywgJ3BhY2thZ2UuanNvbicsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1BhdGggdG8gYHBhY2thZ2UuanNvbmAnLFxuICB9KVxuXG4gIG5wbURpciA9IE9wdGlvbi5TdHJpbmcoJy0tbnBtLWRpcicsICducG0nLCB7XG4gICAgZGVzY3JpcHRpb246ICdQYXRoIHRvIHRoZSBmb2xkZXIgd2hlcmUgdGhlIG5wbSBwYWNrYWdlcyBwdXQnLFxuICB9KVxuXG4gIGdldE9wdGlvbnMoKSB7XG4gICAgcmV0dXJuIHtcbiAgICAgIGN3ZDogdGhpcy5jd2QsXG4gICAgICBjb25maWdQYXRoOiB0aGlzLmNvbmZpZ1BhdGgsXG4gICAgICBwYWNrYWdlSnNvblBhdGg6IHRoaXMucGFja2FnZUpzb25QYXRoLFxuICAgICAgbnBtRGlyOiB0aGlzLm5wbURpcixcbiAgICB9XG4gIH1cbn1cblxuLyoqXG4gKiBVcGRhdGUgdmVyc2lvbiBpbiBjcmVhdGVkIG5wbSBwYWNrYWdlc1xuICovXG5leHBvcnQgaW50ZXJmYWNlIFZlcnNpb25PcHRpb25zIHtcbiAgLyoqXG4gICAqIFRoZSB3b3JraW5nIGRpcmVjdG9yeSBvZiB3aGVyZSBuYXBpIGNvbW1hbmQgd2lsbCBiZSBleGVjdXRlZCBpbiwgYWxsIG90aGVyIHBhdGhzIG9wdGlvbnMgYXJlIHJlbGF0aXZlIHRvIHRoaXMgcGF0aFxuICAgKlxuICAgKiBAZGVmYXVsdCBwcm9jZXNzLmN3ZCgpXG4gICAqL1xuICBjd2Q/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gYG5hcGlgIGNvbmZpZyBqc29uIGZpbGVcbiAgICovXG4gIGNvbmZpZ1BhdGg/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gYHBhY2thZ2UuanNvbmBcbiAgICpcbiAgICogQGRlZmF1bHQgJ3BhY2thZ2UuanNvbidcbiAgICovXG4gIHBhY2thZ2VKc29uUGF0aD86IHN0cmluZ1xuICAvKipcbiAgICogUGF0aCB0byB0aGUgZm9sZGVyIHdoZXJlIHRoZSBucG0gcGFja2FnZXMgcHV0XG4gICAqXG4gICAqIEBkZWZhdWx0ICducG0nXG4gICAqL1xuICBucG1EaXI/OiBzdHJpbmdcbn1cblxuZXhwb3J0IGZ1bmN0aW9uIGFwcGx5RGVmYXVsdFZlcnNpb25PcHRpb25zKG9wdGlvbnM6IFZlcnNpb25PcHRpb25zKSB7XG4gIHJldHVybiB7XG4gICAgY3dkOiBwcm9jZXNzLmN3ZCgpLFxuICAgIHBhY2thZ2VKc29uUGF0aDogJ3BhY2thZ2UuanNvbicsXG4gICAgbnBtRGlyOiAnbnBtJyxcbiAgICAuLi5vcHRpb25zLFxuICB9XG59XG4iLCJpbXBvcnQgeyBqb2luLCByZXNvbHZlIH0gZnJvbSAnbm9kZTpwYXRoJ1xuXG5pbXBvcnQge1xuICBhcHBseURlZmF1bHRWZXJzaW9uT3B0aW9ucyxcbiAgdHlwZSBWZXJzaW9uT3B0aW9ucyxcbn0gZnJvbSAnLi4vZGVmL3ZlcnNpb24uanMnXG5pbXBvcnQge1xuICByZWFkTmFwaUNvbmZpZyxcbiAgZGVidWdGYWN0b3J5LFxuICB1cGRhdGVQYWNrYWdlSnNvbixcbn0gZnJvbSAnLi4vdXRpbHMvaW5kZXguanMnXG5cbmNvbnN0IGRlYnVnID0gZGVidWdGYWN0b3J5KCd2ZXJzaW9uJylcblxuZXhwb3J0IGFzeW5jIGZ1bmN0aW9uIHZlcnNpb24odXNlck9wdGlvbnM6IFZlcnNpb25PcHRpb25zKSB7XG4gIGNvbnN0IG9wdGlvbnMgPSBhcHBseURlZmF1bHRWZXJzaW9uT3B0aW9ucyh1c2VyT3B0aW9ucylcbiAgY29uc3QgcGFja2FnZUpzb25QYXRoID0gcmVzb2x2ZShvcHRpb25zLmN3ZCwgb3B0aW9ucy5wYWNrYWdlSnNvblBhdGgpXG5cbiAgY29uc3QgY29uZmlnID0gYXdhaXQgcmVhZE5hcGlDb25maWcoXG4gICAgcGFja2FnZUpzb25QYXRoLFxuICAgIG9wdGlvbnMuY29uZmlnUGF0aCA/IHJlc29sdmUob3B0aW9ucy5jd2QsIG9wdGlvbnMuY29uZmlnUGF0aCkgOiB1bmRlZmluZWQsXG4gIClcblxuICBmb3IgKGNvbnN0IHRhcmdldCBvZiBjb25maWcudGFyZ2V0cykge1xuICAgIGNvbnN0IHBrZ0RpciA9IHJlc29sdmUob3B0aW9ucy5jd2QsIG9wdGlvbnMubnBtRGlyLCB0YXJnZXQucGxhdGZvcm1BcmNoQUJJKVxuXG4gICAgZGVidWcoYFVwZGF0ZSB2ZXJzaW9uIHRvICVpIGluIFslaV1gLCBjb25maWcucGFja2FnZUpzb24udmVyc2lvbiwgcGtnRGlyKVxuICAgIGF3YWl0IHVwZGF0ZVBhY2thZ2VKc29uKGpvaW4ocGtnRGlyLCAncGFja2FnZS5qc29uJyksIHtcbiAgICAgIHZlcnNpb246IGNvbmZpZy5wYWNrYWdlSnNvbi52ZXJzaW9uLFxuICAgIH0pXG4gIH1cbn1cbiIsImltcG9ydCB7IGV4ZWNTeW5jIH0gZnJvbSAnbm9kZTpjaGlsZF9wcm9jZXNzJ1xuaW1wb3J0IHsgZXhpc3RzU3luYywgc3RhdFN5bmMgfSBmcm9tICdub2RlOmZzJ1xuaW1wb3J0IHsgam9pbiwgcmVzb2x2ZSB9IGZyb20gJ25vZGU6cGF0aCdcblxuaW1wb3J0IHsgT2N0b2tpdCB9IGZyb20gJ0BvY3Rva2l0L3Jlc3QnXG5cbmltcG9ydCB7XG4gIGFwcGx5RGVmYXVsdFByZVB1Ymxpc2hPcHRpb25zLFxuICB0eXBlIFByZVB1Ymxpc2hPcHRpb25zLFxufSBmcm9tICcuLi9kZWYvcHJlLXB1Ymxpc2guanMnXG5pbXBvcnQge1xuICByZWFkRmlsZUFzeW5jLFxuICByZWFkTmFwaUNvbmZpZyxcbiAgZGVidWdGYWN0b3J5LFxuICB1cGRhdGVQYWNrYWdlSnNvbixcbn0gZnJvbSAnLi4vdXRpbHMvaW5kZXguanMnXG5cbmltcG9ydCB7IHZlcnNpb24gfSBmcm9tICcuL3ZlcnNpb24uanMnXG5cbmNvbnN0IGRlYnVnID0gZGVidWdGYWN0b3J5KCdwcmUtcHVibGlzaCcpXG5cbmludGVyZmFjZSBQYWNrYWdlSW5mbyB7XG4gIG5hbWU6IHN0cmluZ1xuICB2ZXJzaW9uOiBzdHJpbmdcbiAgdGFnOiBzdHJpbmdcbn1cblxuZXhwb3J0IGFzeW5jIGZ1bmN0aW9uIHByZVB1Ymxpc2godXNlck9wdGlvbnM6IFByZVB1Ymxpc2hPcHRpb25zKSB7XG4gIGRlYnVnKCdSZWNlaXZlIHByZS1wdWJsaXNoIG9wdGlvbnM6JylcbiAgZGVidWcoJyAgJU8nLCB1c2VyT3B0aW9ucylcblxuICBjb25zdCBvcHRpb25zID0gYXBwbHlEZWZhdWx0UHJlUHVibGlzaE9wdGlvbnModXNlck9wdGlvbnMpXG5cbiAgY29uc3QgcGFja2FnZUpzb25QYXRoID0gcmVzb2x2ZShvcHRpb25zLmN3ZCwgb3B0aW9ucy5wYWNrYWdlSnNvblBhdGgpXG5cbiAgY29uc3QgeyBwYWNrYWdlSnNvbiwgdGFyZ2V0cywgcGFja2FnZU5hbWUsIGJpbmFyeU5hbWUsIG5wbUNsaWVudCB9ID1cbiAgICBhd2FpdCByZWFkTmFwaUNvbmZpZyhcbiAgICAgIHBhY2thZ2VKc29uUGF0aCxcbiAgICAgIG9wdGlvbnMuY29uZmlnUGF0aCA/IHJlc29sdmUob3B0aW9ucy5jd2QsIG9wdGlvbnMuY29uZmlnUGF0aCkgOiB1bmRlZmluZWQsXG4gICAgKVxuXG4gIGFzeW5jIGZ1bmN0aW9uIGNyZWF0ZUdoUmVsZWFzZShwYWNrYWdlTmFtZTogc3RyaW5nLCB2ZXJzaW9uOiBzdHJpbmcpIHtcbiAgICBpZiAoIW9wdGlvbnMuZ2hSZWxlYXNlKSB7XG4gICAgICByZXR1cm4ge1xuICAgICAgICBvd25lcjogbnVsbCxcbiAgICAgICAgcmVwbzogbnVsbCxcbiAgICAgICAgcGtnSW5mbzogeyBuYW1lOiBudWxsLCB2ZXJzaW9uOiBudWxsLCB0YWc6IG51bGwgfSxcbiAgICAgIH1cbiAgICB9XG4gICAgY29uc3QgeyByZXBvLCBvd25lciwgcGtnSW5mbywgb2N0b2tpdCB9ID0gZ2V0UmVwb0luZm8ocGFja2FnZU5hbWUsIHZlcnNpb24pXG5cbiAgICBpZiAoIXJlcG8gfHwgIW93bmVyKSB7XG4gICAgICByZXR1cm4ge1xuICAgICAgICBvd25lcjogbnVsbCxcbiAgICAgICAgcmVwbzogbnVsbCxcbiAgICAgICAgcGtnSW5mbzogeyBuYW1lOiBudWxsLCB2ZXJzaW9uOiBudWxsLCB0YWc6IG51bGwgfSxcbiAgICAgIH1cbiAgICB9XG5cbiAgICBpZiAoIW9wdGlvbnMuZHJ5UnVuKSB7XG4gICAgICB0cnkge1xuICAgICAgICBhd2FpdCBvY3Rva2l0LnJlcG9zLmNyZWF0ZVJlbGVhc2Uoe1xuICAgICAgICAgIG93bmVyLFxuICAgICAgICAgIHJlcG8sXG4gICAgICAgICAgdGFnX25hbWU6IHBrZ0luZm8udGFnLFxuICAgICAgICAgIG5hbWU6IG9wdGlvbnMuZ2hSZWxlYXNlTmFtZSxcbiAgICAgICAgICBwcmVyZWxlYXNlOlxuICAgICAgICAgICAgdmVyc2lvbi5pbmNsdWRlcygnYWxwaGEnKSB8fFxuICAgICAgICAgICAgdmVyc2lvbi5pbmNsdWRlcygnYmV0YScpIHx8XG4gICAgICAgICAgICB2ZXJzaW9uLmluY2x1ZGVzKCdyYycpLFxuICAgICAgICB9KVxuICAgICAgfSBjYXRjaCAoZSkge1xuICAgICAgICBkZWJ1ZyhcbiAgICAgICAgICBgUGFyYW1zOiAke0pTT04uc3RyaW5naWZ5KFxuICAgICAgICAgICAgeyBvd25lciwgcmVwbywgdGFnX25hbWU6IHBrZ0luZm8udGFnIH0sXG4gICAgICAgICAgICBudWxsLFxuICAgICAgICAgICAgMixcbiAgICAgICAgICApfWAsXG4gICAgICAgIClcbiAgICAgICAgY29uc29sZS5lcnJvcihlKVxuICAgICAgfVxuICAgIH1cbiAgICByZXR1cm4geyBvd25lciwgcmVwbywgcGtnSW5mbywgb2N0b2tpdCB9XG4gIH1cblxuICBmdW5jdGlvbiBnZXRSZXBvSW5mbyhwYWNrYWdlTmFtZTogc3RyaW5nLCB2ZXJzaW9uOiBzdHJpbmcpIHtcbiAgICBjb25zdCBoZWFkQ29tbWl0ID0gZXhlY1N5bmMoJ2dpdCBsb2cgLTEgLS1wcmV0dHk9JUInLCB7XG4gICAgICBlbmNvZGluZzogJ3V0Zi04JyxcbiAgICB9KS50cmltKClcblxuICAgIGNvbnN0IHsgR0lUSFVCX1JFUE9TSVRPUlkgfSA9IHByb2Nlc3MuZW52XG4gICAgaWYgKCFHSVRIVUJfUkVQT1NJVE9SWSkge1xuICAgICAgcmV0dXJuIHtcbiAgICAgICAgb3duZXI6IG51bGwsXG4gICAgICAgIHJlcG86IG51bGwsXG4gICAgICAgIHBrZ0luZm86IHsgbmFtZTogbnVsbCwgdmVyc2lvbjogbnVsbCwgdGFnOiBudWxsIH0sXG4gICAgICB9XG4gICAgfVxuICAgIGRlYnVnKGBHaXRodWIgcmVwb3NpdG9yeTogJHtHSVRIVUJfUkVQT1NJVE9SWX1gKVxuICAgIGNvbnN0IFtvd25lciwgcmVwb10gPSBHSVRIVUJfUkVQT1NJVE9SWS5zcGxpdCgnLycpXG4gICAgY29uc3Qgb2N0b2tpdCA9IG5ldyBPY3Rva2l0KHtcbiAgICAgIGF1dGg6IHByb2Nlc3MuZW52LkdJVEhVQl9UT0tFTixcbiAgICB9KVxuICAgIGxldCBwa2dJbmZvOiBQYWNrYWdlSW5mbyB8IHVuZGVmaW5lZFxuICAgIGlmIChvcHRpb25zLnRhZ1N0eWxlID09PSAnbGVybmEnKSB7XG4gICAgICBjb25zdCBwYWNrYWdlc1RvUHVibGlzaCA9IGhlYWRDb21taXRcbiAgICAgICAgLnNwbGl0KCdcXG4nKVxuICAgICAgICAubWFwKChsaW5lKSA9PiBsaW5lLnRyaW0oKSlcbiAgICAgICAgLmZpbHRlcigobGluZSwgaW5kZXgpID0+IGxpbmUubGVuZ3RoICYmIGluZGV4KVxuICAgICAgICAubWFwKChsaW5lKSA9PiBsaW5lLnN1YnN0cmluZygyKSlcbiAgICAgICAgLm1hcChwYXJzZVRhZylcblxuICAgICAgcGtnSW5mbyA9IHBhY2thZ2VzVG9QdWJsaXNoLmZpbmQoXG4gICAgICAgIChwa2dJbmZvKSA9PiBwa2dJbmZvLm5hbWUgPT09IHBhY2thZ2VOYW1lLFxuICAgICAgKVxuXG4gICAgICBpZiAoIXBrZ0luZm8pIHtcbiAgICAgICAgdGhyb3cgbmV3IFR5cGVFcnJvcihcbiAgICAgICAgICBgTm8gcmVsZWFzZSBjb21taXQgZm91bmQgd2l0aCAke3BhY2thZ2VOYW1lfSwgb3JpZ2luYWwgY29tbWl0IGluZm86ICR7aGVhZENvbW1pdH1gLFxuICAgICAgICApXG4gICAgICB9XG4gICAgfSBlbHNlIHtcbiAgICAgIHBrZ0luZm8gPSB7XG4gICAgICAgIHRhZzogYHYke3ZlcnNpb259YCxcbiAgICAgICAgdmVyc2lvbixcbiAgICAgICAgbmFtZTogcGFja2FnZU5hbWUsXG4gICAgICB9XG4gICAgfVxuICAgIHJldHVybiB7IG93bmVyLCByZXBvLCBwa2dJbmZvLCBvY3Rva2l0IH1cbiAgfVxuXG4gIGlmICghb3B0aW9ucy5kcnlSdW4pIHtcbiAgICBhd2FpdCB2ZXJzaW9uKHVzZXJPcHRpb25zKVxuICAgIGF3YWl0IHVwZGF0ZVBhY2thZ2VKc29uKHBhY2thZ2VKc29uUGF0aCwge1xuICAgICAgb3B0aW9uYWxEZXBlbmRlbmNpZXM6IHRhcmdldHMucmVkdWNlKFxuICAgICAgICAoZGVwcywgdGFyZ2V0KSA9PiB7XG4gICAgICAgICAgZGVwc1tgJHtwYWNrYWdlTmFtZX0tJHt0YXJnZXQucGxhdGZvcm1BcmNoQUJJfWBdID0gcGFja2FnZUpzb24udmVyc2lvblxuXG4gICAgICAgICAgcmV0dXJuIGRlcHNcbiAgICAgICAgfSxcbiAgICAgICAge30gYXMgUmVjb3JkPHN0cmluZywgc3RyaW5nPixcbiAgICAgICksXG4gICAgfSlcbiAgfVxuXG4gIGNvbnN0IHsgb3duZXIsIHJlcG8sIHBrZ0luZm8sIG9jdG9raXQgfSA9IG9wdGlvbnMuZ2hSZWxlYXNlSWRcbiAgICA/IGdldFJlcG9JbmZvKHBhY2thZ2VOYW1lLCBwYWNrYWdlSnNvbi52ZXJzaW9uKVxuICAgIDogYXdhaXQgY3JlYXRlR2hSZWxlYXNlKHBhY2thZ2VOYW1lLCBwYWNrYWdlSnNvbi52ZXJzaW9uKVxuXG4gIGZvciAoY29uc3QgdGFyZ2V0IG9mIHRhcmdldHMpIHtcbiAgICBjb25zdCBwa2dEaXIgPSByZXNvbHZlKFxuICAgICAgb3B0aW9ucy5jd2QsXG4gICAgICBvcHRpb25zLm5wbURpcixcbiAgICAgIGAke3RhcmdldC5wbGF0Zm9ybUFyY2hBQkl9YCxcbiAgICApXG4gICAgY29uc3QgZXh0ID1cbiAgICAgIHRhcmdldC5wbGF0Zm9ybSA9PT0gJ3dhc2knIHx8IHRhcmdldC5wbGF0Zm9ybSA9PT0gJ3dhc20nID8gJ3dhc20nIDogJ25vZGUnXG4gICAgY29uc3QgZmlsZW5hbWUgPSBgJHtiaW5hcnlOYW1lfS4ke3RhcmdldC5wbGF0Zm9ybUFyY2hBQkl9LiR7ZXh0fWBcbiAgICBjb25zdCBkc3RQYXRoID0gam9pbihwa2dEaXIsIGZpbGVuYW1lKVxuXG4gICAgaWYgKCFvcHRpb25zLmRyeVJ1bikge1xuICAgICAgaWYgKCFleGlzdHNTeW5jKGRzdFBhdGgpKSB7XG4gICAgICAgIGRlYnVnLndhcm4oYCVzIGRvZXNuJ3QgZXhpc3RgLCBkc3RQYXRoKVxuICAgICAgICBjb250aW51ZVxuICAgICAgfVxuXG4gICAgICBpZiAoIW9wdGlvbnMuc2tpcE9wdGlvbmFsUHVibGlzaCkge1xuICAgICAgICB0cnkge1xuICAgICAgICAgIGNvbnN0IG91dHB1dCA9IGV4ZWNTeW5jKGAke25wbUNsaWVudH0gcHVibGlzaGAsIHtcbiAgICAgICAgICAgIGN3ZDogcGtnRGlyLFxuICAgICAgICAgICAgZW52OiBwcm9jZXNzLmVudixcbiAgICAgICAgICAgIHN0ZGlvOiAncGlwZScsXG4gICAgICAgICAgfSlcbiAgICAgICAgICBwcm9jZXNzLnN0ZG91dC53cml0ZShvdXRwdXQpXG4gICAgICAgIH0gY2F0Y2ggKGUpIHtcbiAgICAgICAgICBpZiAoXG4gICAgICAgICAgICBlIGluc3RhbmNlb2YgRXJyb3IgJiZcbiAgICAgICAgICAgIGUubWVzc2FnZS5pbmNsdWRlcyhcbiAgICAgICAgICAgICAgJ1lvdSBjYW5ub3QgcHVibGlzaCBvdmVyIHRoZSBwcmV2aW91c2x5IHB1Ymxpc2hlZCB2ZXJzaW9ucycsXG4gICAgICAgICAgICApXG4gICAgICAgICAgKSB7XG4gICAgICAgICAgICBjb25zb2xlLmluZm8oZS5tZXNzYWdlKVxuICAgICAgICAgICAgZGVidWcud2FybihgJHtwa2dEaXJ9IGhhcyBiZWVuIHB1Ymxpc2hlZCwgc2tpcHBpbmdgKVxuICAgICAgICAgIH0gZWxzZSB7XG4gICAgICAgICAgICB0aHJvdyBlXG4gICAgICAgICAgfVxuICAgICAgICB9XG4gICAgICB9XG5cbiAgICAgIGlmIChvcHRpb25zLmdoUmVsZWFzZSAmJiByZXBvICYmIG93bmVyKSB7XG4gICAgICAgIGRlYnVnLmluZm8oYENyZWF0aW5nIEdpdEh1YiByZWxlYXNlICR7cGtnSW5mby50YWd9YClcbiAgICAgICAgdHJ5IHtcbiAgICAgICAgICBjb25zdCByZWxlYXNlSWQgPSBvcHRpb25zLmdoUmVsZWFzZUlkXG4gICAgICAgICAgICA/IE51bWJlcihvcHRpb25zLmdoUmVsZWFzZUlkKVxuICAgICAgICAgICAgOiAoXG4gICAgICAgICAgICAgICAgYXdhaXQgb2N0b2tpdCEucmVwb3MuZ2V0UmVsZWFzZUJ5VGFnKHtcbiAgICAgICAgICAgICAgICAgIHJlcG86IHJlcG8sXG4gICAgICAgICAgICAgICAgICBvd25lcjogb3duZXIsXG4gICAgICAgICAgICAgICAgICB0YWc6IHBrZ0luZm8udGFnLFxuICAgICAgICAgICAgICAgIH0pXG4gICAgICAgICAgICAgICkuZGF0YS5pZFxuICAgICAgICAgIGNvbnN0IGRzdEZpbGVTdGF0cyA9IHN0YXRTeW5jKGRzdFBhdGgpXG4gICAgICAgICAgY29uc3QgYXNzZXRJbmZvID0gYXdhaXQgb2N0b2tpdCEucmVwb3MudXBsb2FkUmVsZWFzZUFzc2V0KHtcbiAgICAgICAgICAgIG93bmVyOiBvd25lcixcbiAgICAgICAgICAgIHJlcG86IHJlcG8sXG4gICAgICAgICAgICBuYW1lOiBmaWxlbmFtZSxcbiAgICAgICAgICAgIHJlbGVhc2VfaWQ6IHJlbGVhc2VJZCxcbiAgICAgICAgICAgIG1lZGlhVHlwZTogeyBmb3JtYXQ6ICdyYXcnIH0sXG4gICAgICAgICAgICBoZWFkZXJzOiB7XG4gICAgICAgICAgICAgICdjb250ZW50LWxlbmd0aCc6IGRzdEZpbGVTdGF0cy5zaXplLFxuICAgICAgICAgICAgICAnY29udGVudC10eXBlJzogJ2FwcGxpY2F0aW9uL29jdGV0LXN0cmVhbScsXG4gICAgICAgICAgICB9LFxuICAgICAgICAgICAgLy8gQHRzLWV4cGVjdC1lcnJvciBvY3Rva2l0IHR5cGVzIGFyZSB3cm9uZ1xuICAgICAgICAgICAgZGF0YTogYXdhaXQgcmVhZEZpbGVBc3luYyhkc3RQYXRoKSxcbiAgICAgICAgICB9KVxuICAgICAgICAgIGRlYnVnLmluZm8oYEdpdEh1YiByZWxlYXNlIGNyZWF0ZWRgKVxuICAgICAgICAgIGRlYnVnLmluZm8oYERvd25sb2FkIFVSTDogJXNgLCBhc3NldEluZm8uZGF0YS5icm93c2VyX2Rvd25sb2FkX3VybClcbiAgICAgICAgfSBjYXRjaCAoZSkge1xuICAgICAgICAgIGRlYnVnLmVycm9yKFxuICAgICAgICAgICAgYFBhcmFtOiAke0pTT04uc3RyaW5naWZ5KFxuICAgICAgICAgICAgICB7IG93bmVyLCByZXBvLCB0YWc6IHBrZ0luZm8udGFnLCBmaWxlbmFtZTogZHN0UGF0aCB9LFxuICAgICAgICAgICAgICBudWxsLFxuICAgICAgICAgICAgICAyLFxuICAgICAgICAgICAgKX1gLFxuICAgICAgICAgIClcbiAgICAgICAgICBkZWJ1Zy5lcnJvcihlKVxuICAgICAgICB9XG4gICAgICB9XG4gICAgfVxuICB9XG59XG5cbmZ1bmN0aW9uIHBhcnNlVGFnKHRhZzogc3RyaW5nKSB7XG4gIGNvbnN0IHNlZ21lbnRzID0gdGFnLnNwbGl0KCdAJylcbiAgY29uc3QgdmVyc2lvbiA9IHNlZ21lbnRzLnBvcCgpIVxuICBjb25zdCBuYW1lID0gc2VnbWVudHMuam9pbignQCcpXG5cbiAgcmV0dXJuIHtcbiAgICBuYW1lLFxuICAgIHZlcnNpb24sXG4gICAgdGFnLFxuICB9XG59XG4iLCIvLyBUaGlzIGZpbGUgaXMgZ2VuZXJhdGVkIGJ5IGNvZGVnZW4vaW5kZXgudHNcbi8vIERvIG5vdCBlZGl0IHRoaXMgZmlsZSBtYW51YWxseVxuaW1wb3J0IHsgQ29tbWFuZCwgT3B0aW9uIH0gZnJvbSAnY2xpcGFuaW9uJ1xuXG5leHBvcnQgYWJzdHJhY3QgY2xhc3MgQmFzZVVuaXZlcnNhbGl6ZUNvbW1hbmQgZXh0ZW5kcyBDb21tYW5kIHtcbiAgc3RhdGljIHBhdGhzID0gW1sndW5pdmVyc2FsaXplJ11dXG5cbiAgc3RhdGljIHVzYWdlID0gQ29tbWFuZC5Vc2FnZSh7XG4gICAgZGVzY3JpcHRpb246ICdDb21iaWxlIGJ1aWx0IGJpbmFyaWVzIGludG8gb25lIHVuaXZlcnNhbCBiaW5hcnknLFxuICB9KVxuXG4gIGN3ZCA9IE9wdGlvbi5TdHJpbmcoJy0tY3dkJywgcHJvY2Vzcy5jd2QoKSwge1xuICAgIGRlc2NyaXB0aW9uOlxuICAgICAgJ1RoZSB3b3JraW5nIGRpcmVjdG9yeSBvZiB3aGVyZSBuYXBpIGNvbW1hbmQgd2lsbCBiZSBleGVjdXRlZCBpbiwgYWxsIG90aGVyIHBhdGhzIG9wdGlvbnMgYXJlIHJlbGF0aXZlIHRvIHRoaXMgcGF0aCcsXG4gIH0pXG5cbiAgY29uZmlnUGF0aD86IHN0cmluZyA9IE9wdGlvbi5TdHJpbmcoJy0tY29uZmlnLXBhdGgsLWMnLCB7XG4gICAgZGVzY3JpcHRpb246ICdQYXRoIHRvIGBuYXBpYCBjb25maWcganNvbiBmaWxlJyxcbiAgfSlcblxuICBwYWNrYWdlSnNvblBhdGggPSBPcHRpb24uU3RyaW5nKCctLXBhY2thZ2UtanNvbi1wYXRoJywgJ3BhY2thZ2UuanNvbicsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1BhdGggdG8gYHBhY2thZ2UuanNvbmAnLFxuICB9KVxuXG4gIG91dHB1dERpciA9IE9wdGlvbi5TdHJpbmcoJy0tb3V0cHV0LWRpciwtbycsICcuLycsIHtcbiAgICBkZXNjcmlwdGlvbjpcbiAgICAgICdQYXRoIHRvIHRoZSBmb2xkZXIgd2hlcmUgYWxsIGJ1aWx0IGAubm9kZWAgZmlsZXMgcHV0LCBzYW1lIGFzIGAtLW91dHB1dC1kaXJgIG9mIGJ1aWxkIGNvbW1hbmQnLFxuICB9KVxuXG4gIGdldE9wdGlvbnMoKSB7XG4gICAgcmV0dXJuIHtcbiAgICAgIGN3ZDogdGhpcy5jd2QsXG4gICAgICBjb25maWdQYXRoOiB0aGlzLmNvbmZpZ1BhdGgsXG4gICAgICBwYWNrYWdlSnNvblBhdGg6IHRoaXMucGFja2FnZUpzb25QYXRoLFxuICAgICAgb3V0cHV0RGlyOiB0aGlzLm91dHB1dERpcixcbiAgICB9XG4gIH1cbn1cblxuLyoqXG4gKiBDb21iaWxlIGJ1aWx0IGJpbmFyaWVzIGludG8gb25lIHVuaXZlcnNhbCBiaW5hcnlcbiAqL1xuZXhwb3J0IGludGVyZmFjZSBVbml2ZXJzYWxpemVPcHRpb25zIHtcbiAgLyoqXG4gICAqIFRoZSB3b3JraW5nIGRpcmVjdG9yeSBvZiB3aGVyZSBuYXBpIGNvbW1hbmQgd2lsbCBiZSBleGVjdXRlZCBpbiwgYWxsIG90aGVyIHBhdGhzIG9wdGlvbnMgYXJlIHJlbGF0aXZlIHRvIHRoaXMgcGF0aFxuICAgKlxuICAgKiBAZGVmYXVsdCBwcm9jZXNzLmN3ZCgpXG4gICAqL1xuICBjd2Q/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gYG5hcGlgIGNvbmZpZyBqc29uIGZpbGVcbiAgICovXG4gIGNvbmZpZ1BhdGg/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gYHBhY2thZ2UuanNvbmBcbiAgICpcbiAgICogQGRlZmF1bHQgJ3BhY2thZ2UuanNvbidcbiAgICovXG4gIHBhY2thZ2VKc29uUGF0aD86IHN0cmluZ1xuICAvKipcbiAgICogUGF0aCB0byB0aGUgZm9sZGVyIHdoZXJlIGFsbCBidWlsdCBgLm5vZGVgIGZpbGVzIHB1dCwgc2FtZSBhcyBgLS1vdXRwdXQtZGlyYCBvZiBidWlsZCBjb21tYW5kXG4gICAqXG4gICAqIEBkZWZhdWx0ICcuLydcbiAgICovXG4gIG91dHB1dERpcj86IHN0cmluZ1xufVxuXG5leHBvcnQgZnVuY3Rpb24gYXBwbHlEZWZhdWx0VW5pdmVyc2FsaXplT3B0aW9ucyhvcHRpb25zOiBVbml2ZXJzYWxpemVPcHRpb25zKSB7XG4gIHJldHVybiB7XG4gICAgY3dkOiBwcm9jZXNzLmN3ZCgpLFxuICAgIHBhY2thZ2VKc29uUGF0aDogJ3BhY2thZ2UuanNvbicsXG4gICAgb3V0cHV0RGlyOiAnLi8nLFxuICAgIC4uLm9wdGlvbnMsXG4gIH1cbn1cbiIsImltcG9ydCB7IHNwYXduU3luYyB9IGZyb20gJ25vZGU6Y2hpbGRfcHJvY2VzcydcbmltcG9ydCB7IGpvaW4sIHJlc29sdmUgfSBmcm9tICdub2RlOnBhdGgnXG5cbmltcG9ydCB7XG4gIGFwcGx5RGVmYXVsdFVuaXZlcnNhbGl6ZU9wdGlvbnMsXG4gIHR5cGUgVW5pdmVyc2FsaXplT3B0aW9ucyxcbn0gZnJvbSAnLi4vZGVmL3VuaXZlcnNhbGl6ZS5qcydcbmltcG9ydCB7IHJlYWROYXBpQ29uZmlnIH0gZnJvbSAnLi4vdXRpbHMvY29uZmlnLmpzJ1xuaW1wb3J0IHsgZGVidWdGYWN0b3J5IH0gZnJvbSAnLi4vdXRpbHMvbG9nLmpzJ1xuaW1wb3J0IHsgZmlsZUV4aXN0cyB9IGZyb20gJy4uL3V0aWxzL21pc2MuanMnXG5pbXBvcnQgeyBVbmlBcmNoc0J5UGxhdGZvcm0gfSBmcm9tICcuLi91dGlscy90YXJnZXQuanMnXG5cbmNvbnN0IGRlYnVnID0gZGVidWdGYWN0b3J5KCd1bml2ZXJzYWxpemUnKVxuXG5jb25zdCB1bml2ZXJzYWxpemVyczogUGFydGlhbDxcbiAgUmVjb3JkPE5vZGVKUy5QbGF0Zm9ybSwgKGlucHV0czogc3RyaW5nW10sIG91dHB1dDogc3RyaW5nKSA9PiB2b2lkPlxuPiA9IHtcbiAgZGFyd2luOiAoaW5wdXRzLCBvdXRwdXQpID0+IHtcbiAgICBzcGF3blN5bmMoJ2xpcG8nLCBbJy1jcmVhdGUnLCAnLW91dHB1dCcsIG91dHB1dCwgLi4uaW5wdXRzXSwge1xuICAgICAgc3RkaW86ICdpbmhlcml0JyxcbiAgICB9KVxuICB9LFxufVxuXG5leHBvcnQgYXN5bmMgZnVuY3Rpb24gdW5pdmVyc2FsaXplQmluYXJpZXModXNlck9wdGlvbnM6IFVuaXZlcnNhbGl6ZU9wdGlvbnMpIHtcbiAgY29uc3Qgb3B0aW9ucyA9IGFwcGx5RGVmYXVsdFVuaXZlcnNhbGl6ZU9wdGlvbnModXNlck9wdGlvbnMpXG5cbiAgY29uc3QgcGFja2FnZUpzb25QYXRoID0gam9pbihvcHRpb25zLmN3ZCwgb3B0aW9ucy5wYWNrYWdlSnNvblBhdGgpXG5cbiAgY29uc3QgY29uZmlnID0gYXdhaXQgcmVhZE5hcGlDb25maWcoXG4gICAgcGFja2FnZUpzb25QYXRoLFxuICAgIG9wdGlvbnMuY29uZmlnUGF0aCA/IHJlc29sdmUob3B0aW9ucy5jd2QsIG9wdGlvbnMuY29uZmlnUGF0aCkgOiB1bmRlZmluZWQsXG4gIClcblxuICBjb25zdCB0YXJnZXQgPSBjb25maWcudGFyZ2V0cy5maW5kKFxuICAgICh0KSA9PiB0LnBsYXRmb3JtID09PSBwcm9jZXNzLnBsYXRmb3JtICYmIHQuYXJjaCA9PT0gJ3VuaXZlcnNhbCcsXG4gIClcblxuICBpZiAoIXRhcmdldCkge1xuICAgIHRocm93IG5ldyBFcnJvcihcbiAgICAgIGAndW5pdmVyc2FsJyBhcmNoIGZvciBwbGF0Zm9ybSAnJHtwcm9jZXNzLnBsYXRmb3JtfScgbm90IGZvdW5kIGluIGNvbmZpZyFgLFxuICAgIClcbiAgfVxuXG4gIGNvbnN0IHNyY0ZpbGVzID0gVW5pQXJjaHNCeVBsYXRmb3JtW3Byb2Nlc3MucGxhdGZvcm1dPy5tYXAoKGFyY2gpID0+XG4gICAgcmVzb2x2ZShcbiAgICAgIG9wdGlvbnMuY3dkLFxuICAgICAgb3B0aW9ucy5vdXRwdXREaXIsXG4gICAgICBgJHtjb25maWcuYmluYXJ5TmFtZX0uJHtwcm9jZXNzLnBsYXRmb3JtfS0ke2FyY2h9Lm5vZGVgLFxuICAgICksXG4gIClcblxuICBpZiAoIXNyY0ZpbGVzIHx8ICF1bml2ZXJzYWxpemVyc1twcm9jZXNzLnBsYXRmb3JtXSkge1xuICAgIHRocm93IG5ldyBFcnJvcihcbiAgICAgIGAndW5pdmVyc2FsJyBhcmNoIGZvciBwbGF0Zm9ybSAnJHtwcm9jZXNzLnBsYXRmb3JtfScgbm90IHN1cHBvcnRlZC5gLFxuICAgIClcbiAgfVxuXG4gIGRlYnVnKGBMb29raW5nIHVwIHNvdXJjZSBiaW5hcmllcyB0byBjb21iaW5lOiBgKVxuICBkZWJ1ZygnICAlTycsIHNyY0ZpbGVzKVxuXG4gIGNvbnN0IHNyY0ZpbGVMb29rdXAgPSBhd2FpdCBQcm9taXNlLmFsbChzcmNGaWxlcy5tYXAoKGYpID0+IGZpbGVFeGlzdHMoZikpKVxuXG4gIGNvbnN0IG5vdEZvdW5kRmlsZXMgPSBzcmNGaWxlcy5maWx0ZXIoKF8sIGkpID0+ICFzcmNGaWxlTG9va3VwW2ldKVxuXG4gIGlmIChub3RGb3VuZEZpbGVzLmxlbmd0aCkge1xuICAgIHRocm93IG5ldyBFcnJvcihcbiAgICAgIGBTb21lIGJpbmFyeSBmaWxlcyB3ZXJlIG5vdCBmb3VuZDogJHtKU09OLnN0cmluZ2lmeShub3RGb3VuZEZpbGVzKX1gLFxuICAgIClcbiAgfVxuXG4gIGNvbnN0IG91dHB1dCA9IHJlc29sdmUoXG4gICAgb3B0aW9ucy5jd2QsXG4gICAgb3B0aW9ucy5vdXRwdXREaXIsXG4gICAgYCR7Y29uZmlnLmJpbmFyeU5hbWV9LiR7cHJvY2Vzcy5wbGF0Zm9ybX0tdW5pdmVyc2FsLm5vZGVgLFxuICApXG5cbiAgdW5pdmVyc2FsaXplcnNbcHJvY2Vzcy5wbGF0Zm9ybV0/LihzcmNGaWxlcywgb3V0cHV0KVxuXG4gIGRlYnVnKGBQcm9kdWNlZCB1bml2ZXJzYWwgYmluYXJ5OiAke291dHB1dH1gKVxufVxuIiwiaW1wb3J0IHsgQ29tbWFuZCB9IGZyb20gJ2NsaXBhbmlvbidcblxuaW1wb3J0IHsgY29sbGVjdEFydGlmYWN0cyB9IGZyb20gJy4uL2FwaS9hcnRpZmFjdHMuanMnXG5pbXBvcnQgeyBCYXNlQXJ0aWZhY3RzQ29tbWFuZCB9IGZyb20gJy4uL2RlZi9hcnRpZmFjdHMuanMnXG5cbmV4cG9ydCBjbGFzcyBBcnRpZmFjdHNDb21tYW5kIGV4dGVuZHMgQmFzZUFydGlmYWN0c0NvbW1hbmQge1xuICBzdGF0aWMgdXNhZ2UgPSBDb21tYW5kLlVzYWdlKHtcbiAgICBkZXNjcmlwdGlvbjogJ0NvcHkgYXJ0aWZhY3RzIGZyb20gR2l0aHViIEFjdGlvbnMgaW50byBzcGVjaWZpZWQgZGlyJyxcbiAgICBleGFtcGxlczogW1xuICAgICAgW1xuICAgICAgICAnJDAgYXJ0aWZhY3RzIC0tb3V0cHV0LWRpciAuL2FydGlmYWN0cyAtLWRpc3QgLi9ucG0nLFxuICAgICAgICBgQ29weSBbYmluYXJ5TmFtZV0uW3BsYXRmb3JtXS5ub2RlIHVuZGVyIGN1cnJlbnQgZGlyKC4pIGludG8gcGFja2FnZXMgdW5kZXIgbnBtIGRpci5cbmUuZzogaW5kZXgubGludXgteDY0LWdudS5ub2RlIC0tPiAuL25wbS9saW51eC14NjQtZ251L2luZGV4LmxpbnV4LXg2NC1nbnUubm9kZWAsXG4gICAgICBdLFxuICAgIF0sXG4gIH0pXG5cbiAgc3RhdGljIHBhdGhzID0gW1snYXJ0aWZhY3RzJ11dXG5cbiAgYXN5bmMgZXhlY3V0ZSgpIHtcbiAgICBhd2FpdCBjb2xsZWN0QXJ0aWZhY3RzKHRoaXMuZ2V0T3B0aW9ucygpKVxuICB9XG59XG4iLCIvLyBUaGlzIGZpbGUgaXMgZ2VuZXJhdGVkIGJ5IGNvZGVnZW4vaW5kZXgudHNcbi8vIERvIG5vdCBlZGl0IHRoaXMgZmlsZSBtYW51YWxseVxuaW1wb3J0IHsgQ29tbWFuZCwgT3B0aW9uIH0gZnJvbSAnY2xpcGFuaW9uJ1xuXG5leHBvcnQgYWJzdHJhY3QgY2xhc3MgQmFzZUJ1aWxkQ29tbWFuZCBleHRlbmRzIENvbW1hbmQge1xuICBzdGF0aWMgcGF0aHMgPSBbWydidWlsZCddXVxuXG4gIHN0YXRpYyB1c2FnZSA9IENvbW1hbmQuVXNhZ2Uoe1xuICAgIGRlc2NyaXB0aW9uOiAnQnVpbGQgdGhlIE5BUEktUlMgcHJvamVjdCcsXG4gIH0pXG5cbiAgdGFyZ2V0Pzogc3RyaW5nID0gT3B0aW9uLlN0cmluZygnLS10YXJnZXQsLXQnLCB7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnQnVpbGQgZm9yIHRoZSB0YXJnZXQgdHJpcGxlLCBieXBhc3NlZCB0byBgY2FyZ28gYnVpbGQgLS10YXJnZXRgJyxcbiAgfSlcblxuICBjd2Q/OiBzdHJpbmcgPSBPcHRpb24uU3RyaW5nKCctLWN3ZCcsIHtcbiAgICBkZXNjcmlwdGlvbjpcbiAgICAgICdUaGUgd29ya2luZyBkaXJlY3Rvcnkgb2Ygd2hlcmUgbmFwaSBjb21tYW5kIHdpbGwgYmUgZXhlY3V0ZWQgaW4sIGFsbCBvdGhlciBwYXRocyBvcHRpb25zIGFyZSByZWxhdGl2ZSB0byB0aGlzIHBhdGgnLFxuICB9KVxuXG4gIG1hbmlmZXN0UGF0aD86IHN0cmluZyA9IE9wdGlvbi5TdHJpbmcoJy0tbWFuaWZlc3QtcGF0aCcsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1BhdGggdG8gYENhcmdvLnRvbWxgJyxcbiAgfSlcblxuICBjb25maWdQYXRoPzogc3RyaW5nID0gT3B0aW9uLlN0cmluZygnLS1jb25maWctcGF0aCwtYycsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1BhdGggdG8gYG5hcGlgIGNvbmZpZyBqc29uIGZpbGUnLFxuICB9KVxuXG4gIHBhY2thZ2VKc29uUGF0aD86IHN0cmluZyA9IE9wdGlvbi5TdHJpbmcoJy0tcGFja2FnZS1qc29uLXBhdGgnLCB7XG4gICAgZGVzY3JpcHRpb246ICdQYXRoIHRvIGBwYWNrYWdlLmpzb25gJyxcbiAgfSlcblxuICB0YXJnZXREaXI/OiBzdHJpbmcgPSBPcHRpb24uU3RyaW5nKCctLXRhcmdldC1kaXInLCB7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnRGlyZWN0b3J5IGZvciBhbGwgY3JhdGUgZ2VuZXJhdGVkIGFydGlmYWN0cywgc2VlIGBjYXJnbyBidWlsZCAtLXRhcmdldC1kaXJgJyxcbiAgfSlcblxuICBvdXRwdXREaXI/OiBzdHJpbmcgPSBPcHRpb24uU3RyaW5nKCctLW91dHB1dC1kaXIsLW8nLCB7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnUGF0aCB0byB3aGVyZSBhbGwgdGhlIGJ1aWx0IGZpbGVzIHdvdWxkIGJlIHB1dC4gRGVmYXVsdCB0byB0aGUgY3JhdGUgZm9sZGVyJyxcbiAgfSlcblxuICBwbGF0Zm9ybT86IGJvb2xlYW4gPSBPcHRpb24uQm9vbGVhbignLS1wbGF0Zm9ybScsIHtcbiAgICBkZXNjcmlwdGlvbjpcbiAgICAgICdBZGQgcGxhdGZvcm0gdHJpcGxlIHRvIHRoZSBnZW5lcmF0ZWQgbm9kZWpzIGJpbmRpbmcgZmlsZSwgZWc6IGBbbmFtZV0ubGludXgteDY0LWdudS5ub2RlYCcsXG4gIH0pXG5cbiAganNQYWNrYWdlTmFtZT86IHN0cmluZyA9IE9wdGlvbi5TdHJpbmcoJy0tanMtcGFja2FnZS1uYW1lJywge1xuICAgIGRlc2NyaXB0aW9uOlxuICAgICAgJ1BhY2thZ2UgbmFtZSBpbiBnZW5lcmF0ZWQganMgYmluZGluZyBmaWxlLiBPbmx5IHdvcmtzIHdpdGggYC0tcGxhdGZvcm1gIGZsYWcnLFxuICB9KVxuXG4gIGNvbnN0RW51bT86IGJvb2xlYW4gPSBPcHRpb24uQm9vbGVhbignLS1jb25zdC1lbnVtJywge1xuICAgIGRlc2NyaXB0aW9uOiAnV2hldGhlciBnZW5lcmF0ZSBjb25zdCBlbnVtIGZvciB0eXBlc2NyaXB0IGJpbmRpbmdzJyxcbiAgfSlcblxuICBqc0JpbmRpbmc/OiBzdHJpbmcgPSBPcHRpb24uU3RyaW5nKCctLWpzJywge1xuICAgIGRlc2NyaXB0aW9uOlxuICAgICAgJ1BhdGggYW5kIGZpbGVuYW1lIG9mIGdlbmVyYXRlZCBKUyBiaW5kaW5nIGZpbGUuIE9ubHkgd29ya3Mgd2l0aCBgLS1wbGF0Zm9ybWAgZmxhZy4gUmVsYXRpdmUgdG8gYC0tb3V0cHV0LWRpcmAuJyxcbiAgfSlcblxuICBub0pzQmluZGluZz86IGJvb2xlYW4gPSBPcHRpb24uQm9vbGVhbignLS1uby1qcycsIHtcbiAgICBkZXNjcmlwdGlvbjpcbiAgICAgICdXaGV0aGVyIHRvIGRpc2FibGUgdGhlIGdlbmVyYXRpb24gSlMgYmluZGluZyBmaWxlLiBPbmx5IHdvcmtzIHdpdGggYC0tcGxhdGZvcm1gIGZsYWcuJyxcbiAgfSlcblxuICBkdHM/OiBzdHJpbmcgPSBPcHRpb24uU3RyaW5nKCctLWR0cycsIHtcbiAgICBkZXNjcmlwdGlvbjpcbiAgICAgICdQYXRoIGFuZCBmaWxlbmFtZSBvZiBnZW5lcmF0ZWQgdHlwZSBkZWYgZmlsZS4gUmVsYXRpdmUgdG8gYC0tb3V0cHV0LWRpcmAnLFxuICB9KVxuXG4gIGR0c0hlYWRlcj86IHN0cmluZyA9IE9wdGlvbi5TdHJpbmcoJy0tZHRzLWhlYWRlcicsIHtcbiAgICBkZXNjcmlwdGlvbjpcbiAgICAgICdDdXN0b20gZmlsZSBoZWFkZXIgZm9yIGdlbmVyYXRlZCB0eXBlIGRlZiBmaWxlLiBPbmx5IHdvcmtzIHdoZW4gYHR5cGVkZWZgIGZlYXR1cmUgZW5hYmxlZC4nLFxuICB9KVxuXG4gIG5vRHRzSGVhZGVyPzogYm9vbGVhbiA9IE9wdGlvbi5Cb29sZWFuKCctLW5vLWR0cy1oZWFkZXInLCB7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnV2hldGhlciB0byBkaXNhYmxlIHRoZSBkZWZhdWx0IGZpbGUgaGVhZGVyIGZvciBnZW5lcmF0ZWQgdHlwZSBkZWYgZmlsZS4gT25seSB3b3JrcyB3aGVuIGB0eXBlZGVmYCBmZWF0dXJlIGVuYWJsZWQuJyxcbiAgfSlcblxuICBkdHNDYWNoZSA9IE9wdGlvbi5Cb29sZWFuKCctLWR0cy1jYWNoZScsIHRydWUsIHtcbiAgICBkZXNjcmlwdGlvbjogJ1doZXRoZXIgdG8gZW5hYmxlIHRoZSBkdHMgY2FjaGUsIGRlZmF1bHQgdG8gdHJ1ZScsXG4gIH0pXG5cbiAgZXNtPzogYm9vbGVhbiA9IE9wdGlvbi5Cb29sZWFuKCctLWVzbScsIHtcbiAgICBkZXNjcmlwdGlvbjpcbiAgICAgICdXaGV0aGVyIHRvIGVtaXQgYW4gRVNNIEpTIGJpbmRpbmcgZmlsZSBpbnN0ZWFkIG9mIENKUyBmb3JtYXQuIE9ubHkgd29ya3Mgd2l0aCBgLS1wbGF0Zm9ybWAgZmxhZy4nLFxuICB9KVxuXG4gIHN0cmlwPzogYm9vbGVhbiA9IE9wdGlvbi5Cb29sZWFuKCctLXN0cmlwLC1zJywge1xuICAgIGRlc2NyaXB0aW9uOiAnV2hldGhlciBzdHJpcCB0aGUgbGlicmFyeSB0byBhY2hpZXZlIHRoZSBtaW5pbXVtIGZpbGUgc2l6ZScsXG4gIH0pXG5cbiAgcmVsZWFzZT86IGJvb2xlYW4gPSBPcHRpb24uQm9vbGVhbignLS1yZWxlYXNlLC1yJywge1xuICAgIGRlc2NyaXB0aW9uOiAnQnVpbGQgaW4gcmVsZWFzZSBtb2RlJyxcbiAgfSlcblxuICB2ZXJib3NlPzogYm9vbGVhbiA9IE9wdGlvbi5Cb29sZWFuKCctLXZlcmJvc2UsLXYnLCB7XG4gICAgZGVzY3JpcHRpb246ICdWZXJib3NlbHkgbG9nIGJ1aWxkIGNvbW1hbmQgdHJhY2UnLFxuICB9KVxuXG4gIGJpbj86IHN0cmluZyA9IE9wdGlvbi5TdHJpbmcoJy0tYmluJywge1xuICAgIGRlc2NyaXB0aW9uOiAnQnVpbGQgb25seSB0aGUgc3BlY2lmaWVkIGJpbmFyeScsXG4gIH0pXG5cbiAgcGFja2FnZT86IHN0cmluZyA9IE9wdGlvbi5TdHJpbmcoJy0tcGFja2FnZSwtcCcsIHtcbiAgICBkZXNjcmlwdGlvbjogJ0J1aWxkIHRoZSBzcGVjaWZpZWQgbGlicmFyeSBvciB0aGUgb25lIGF0IGN3ZCcsXG4gIH0pXG5cbiAgcHJvZmlsZT86IHN0cmluZyA9IE9wdGlvbi5TdHJpbmcoJy0tcHJvZmlsZScsIHtcbiAgICBkZXNjcmlwdGlvbjogJ0J1aWxkIGFydGlmYWN0cyB3aXRoIHRoZSBzcGVjaWZpZWQgcHJvZmlsZScsXG4gIH0pXG5cbiAgY3Jvc3NDb21waWxlPzogYm9vbGVhbiA9IE9wdGlvbi5Cb29sZWFuKCctLWNyb3NzLWNvbXBpbGUsLXgnLCB7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnW2V4cGVyaW1lbnRhbF0gY3Jvc3MtY29tcGlsZSBmb3IgdGhlIHNwZWNpZmllZCB0YXJnZXQgd2l0aCBgY2FyZ28teHdpbmAgb24gd2luZG93cyBhbmQgYGNhcmdvLXppZ2J1aWxkYCBvbiBvdGhlciBwbGF0Zm9ybScsXG4gIH0pXG5cbiAgdXNlQ3Jvc3M/OiBib29sZWFuID0gT3B0aW9uLkJvb2xlYW4oJy0tdXNlLWNyb3NzJywge1xuICAgIGRlc2NyaXB0aW9uOlxuICAgICAgJ1tleHBlcmltZW50YWxdIHVzZSBbY3Jvc3NdKGh0dHBzOi8vZ2l0aHViLmNvbS9jcm9zcy1ycy9jcm9zcykgaW5zdGVhZCBvZiBgY2FyZ29gJyxcbiAgfSlcblxuICB1c2VOYXBpQ3Jvc3M/OiBib29sZWFuID0gT3B0aW9uLkJvb2xlYW4oJy0tdXNlLW5hcGktY3Jvc3MnLCB7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnW2V4cGVyaW1lbnRhbF0gdXNlIEBuYXBpLXJzL2Nyb3NzLXRvb2xjaGFpbiB0byBjcm9zcy1jb21waWxlIExpbnV4IGFybS9hcm02NC94NjQgZ251IHRhcmdldHMuJyxcbiAgfSlcblxuICB3YXRjaD86IGJvb2xlYW4gPSBPcHRpb24uQm9vbGVhbignLS13YXRjaCwtdycsIHtcbiAgICBkZXNjcmlwdGlvbjpcbiAgICAgICd3YXRjaCB0aGUgY3JhdGUgY2hhbmdlcyBhbmQgYnVpbGQgY29udGludW91c2x5IHdpdGggYGNhcmdvLXdhdGNoYCBjcmF0ZXMnLFxuICB9KVxuXG4gIGZlYXR1cmVzPzogc3RyaW5nW10gPSBPcHRpb24uQXJyYXkoJy0tZmVhdHVyZXMsLUYnLCB7XG4gICAgZGVzY3JpcHRpb246ICdTcGFjZS1zZXBhcmF0ZWQgbGlzdCBvZiBmZWF0dXJlcyB0byBhY3RpdmF0ZScsXG4gIH0pXG5cbiAgYWxsRmVhdHVyZXM/OiBib29sZWFuID0gT3B0aW9uLkJvb2xlYW4oJy0tYWxsLWZlYXR1cmVzJywge1xuICAgIGRlc2NyaXB0aW9uOiAnQWN0aXZhdGUgYWxsIGF2YWlsYWJsZSBmZWF0dXJlcycsXG4gIH0pXG5cbiAgbm9EZWZhdWx0RmVhdHVyZXM/OiBib29sZWFuID0gT3B0aW9uLkJvb2xlYW4oJy0tbm8tZGVmYXVsdC1mZWF0dXJlcycsIHtcbiAgICBkZXNjcmlwdGlvbjogJ0RvIG5vdCBhY3RpdmF0ZSB0aGUgYGRlZmF1bHRgIGZlYXR1cmUnLFxuICB9KVxuXG4gIGdldE9wdGlvbnMoKSB7XG4gICAgcmV0dXJuIHtcbiAgICAgIHRhcmdldDogdGhpcy50YXJnZXQsXG4gICAgICBjd2Q6IHRoaXMuY3dkLFxuICAgICAgbWFuaWZlc3RQYXRoOiB0aGlzLm1hbmlmZXN0UGF0aCxcbiAgICAgIGNvbmZpZ1BhdGg6IHRoaXMuY29uZmlnUGF0aCxcbiAgICAgIHBhY2thZ2VKc29uUGF0aDogdGhpcy5wYWNrYWdlSnNvblBhdGgsXG4gICAgICB0YXJnZXREaXI6IHRoaXMudGFyZ2V0RGlyLFxuICAgICAgb3V0cHV0RGlyOiB0aGlzLm91dHB1dERpcixcbiAgICAgIHBsYXRmb3JtOiB0aGlzLnBsYXRmb3JtLFxuICAgICAganNQYWNrYWdlTmFtZTogdGhpcy5qc1BhY2thZ2VOYW1lLFxuICAgICAgY29uc3RFbnVtOiB0aGlzLmNvbnN0RW51bSxcbiAgICAgIGpzQmluZGluZzogdGhpcy5qc0JpbmRpbmcsXG4gICAgICBub0pzQmluZGluZzogdGhpcy5ub0pzQmluZGluZyxcbiAgICAgIGR0czogdGhpcy5kdHMsXG4gICAgICBkdHNIZWFkZXI6IHRoaXMuZHRzSGVhZGVyLFxuICAgICAgbm9EdHNIZWFkZXI6IHRoaXMubm9EdHNIZWFkZXIsXG4gICAgICBkdHNDYWNoZTogdGhpcy5kdHNDYWNoZSxcbiAgICAgIGVzbTogdGhpcy5lc20sXG4gICAgICBzdHJpcDogdGhpcy5zdHJpcCxcbiAgICAgIHJlbGVhc2U6IHRoaXMucmVsZWFzZSxcbiAgICAgIHZlcmJvc2U6IHRoaXMudmVyYm9zZSxcbiAgICAgIGJpbjogdGhpcy5iaW4sXG4gICAgICBwYWNrYWdlOiB0aGlzLnBhY2thZ2UsXG4gICAgICBwcm9maWxlOiB0aGlzLnByb2ZpbGUsXG4gICAgICBjcm9zc0NvbXBpbGU6IHRoaXMuY3Jvc3NDb21waWxlLFxuICAgICAgdXNlQ3Jvc3M6IHRoaXMudXNlQ3Jvc3MsXG4gICAgICB1c2VOYXBpQ3Jvc3M6IHRoaXMudXNlTmFwaUNyb3NzLFxuICAgICAgd2F0Y2g6IHRoaXMud2F0Y2gsXG4gICAgICBmZWF0dXJlczogdGhpcy5mZWF0dXJlcyxcbiAgICAgIGFsbEZlYXR1cmVzOiB0aGlzLmFsbEZlYXR1cmVzLFxuICAgICAgbm9EZWZhdWx0RmVhdHVyZXM6IHRoaXMubm9EZWZhdWx0RmVhdHVyZXMsXG4gICAgfVxuICB9XG59XG5cbi8qKlxuICogQnVpbGQgdGhlIE5BUEktUlMgcHJvamVjdFxuICovXG5leHBvcnQgaW50ZXJmYWNlIEJ1aWxkT3B0aW9ucyB7XG4gIC8qKlxuICAgKiBCdWlsZCBmb3IgdGhlIHRhcmdldCB0cmlwbGUsIGJ5cGFzc2VkIHRvIGBjYXJnbyBidWlsZCAtLXRhcmdldGBcbiAgICovXG4gIHRhcmdldD86IHN0cmluZ1xuICAvKipcbiAgICogVGhlIHdvcmtpbmcgZGlyZWN0b3J5IG9mIHdoZXJlIG5hcGkgY29tbWFuZCB3aWxsIGJlIGV4ZWN1dGVkIGluLCBhbGwgb3RoZXIgcGF0aHMgb3B0aW9ucyBhcmUgcmVsYXRpdmUgdG8gdGhpcyBwYXRoXG4gICAqL1xuICBjd2Q/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gYENhcmdvLnRvbWxgXG4gICAqL1xuICBtYW5pZmVzdFBhdGg/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gYG5hcGlgIGNvbmZpZyBqc29uIGZpbGVcbiAgICovXG4gIGNvbmZpZ1BhdGg/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gYHBhY2thZ2UuanNvbmBcbiAgICovXG4gIHBhY2thZ2VKc29uUGF0aD86IHN0cmluZ1xuICAvKipcbiAgICogRGlyZWN0b3J5IGZvciBhbGwgY3JhdGUgZ2VuZXJhdGVkIGFydGlmYWN0cywgc2VlIGBjYXJnbyBidWlsZCAtLXRhcmdldC1kaXJgXG4gICAqL1xuICB0YXJnZXREaXI/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFBhdGggdG8gd2hlcmUgYWxsIHRoZSBidWlsdCBmaWxlcyB3b3VsZCBiZSBwdXQuIERlZmF1bHQgdG8gdGhlIGNyYXRlIGZvbGRlclxuICAgKi9cbiAgb3V0cHV0RGlyPzogc3RyaW5nXG4gIC8qKlxuICAgKiBBZGQgcGxhdGZvcm0gdHJpcGxlIHRvIHRoZSBnZW5lcmF0ZWQgbm9kZWpzIGJpbmRpbmcgZmlsZSwgZWc6IGBbbmFtZV0ubGludXgteDY0LWdudS5ub2RlYFxuICAgKi9cbiAgcGxhdGZvcm0/OiBib29sZWFuXG4gIC8qKlxuICAgKiBQYWNrYWdlIG5hbWUgaW4gZ2VuZXJhdGVkIGpzIGJpbmRpbmcgZmlsZS4gT25seSB3b3JrcyB3aXRoIGAtLXBsYXRmb3JtYCBmbGFnXG4gICAqL1xuICBqc1BhY2thZ2VOYW1lPzogc3RyaW5nXG4gIC8qKlxuICAgKiBXaGV0aGVyIGdlbmVyYXRlIGNvbnN0IGVudW0gZm9yIHR5cGVzY3JpcHQgYmluZGluZ3NcbiAgICovXG4gIGNvbnN0RW51bT86IGJvb2xlYW5cbiAgLyoqXG4gICAqIFBhdGggYW5kIGZpbGVuYW1lIG9mIGdlbmVyYXRlZCBKUyBiaW5kaW5nIGZpbGUuIE9ubHkgd29ya3Mgd2l0aCBgLS1wbGF0Zm9ybWAgZmxhZy4gUmVsYXRpdmUgdG8gYC0tb3V0cHV0LWRpcmAuXG4gICAqL1xuICBqc0JpbmRpbmc/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFdoZXRoZXIgdG8gZGlzYWJsZSB0aGUgZ2VuZXJhdGlvbiBKUyBiaW5kaW5nIGZpbGUuIE9ubHkgd29ya3Mgd2l0aCBgLS1wbGF0Zm9ybWAgZmxhZy5cbiAgICovXG4gIG5vSnNCaW5kaW5nPzogYm9vbGVhblxuICAvKipcbiAgICogUGF0aCBhbmQgZmlsZW5hbWUgb2YgZ2VuZXJhdGVkIHR5cGUgZGVmIGZpbGUuIFJlbGF0aXZlIHRvIGAtLW91dHB1dC1kaXJgXG4gICAqL1xuICBkdHM/OiBzdHJpbmdcbiAgLyoqXG4gICAqIEN1c3RvbSBmaWxlIGhlYWRlciBmb3IgZ2VuZXJhdGVkIHR5cGUgZGVmIGZpbGUuIE9ubHkgd29ya3Mgd2hlbiBgdHlwZWRlZmAgZmVhdHVyZSBlbmFibGVkLlxuICAgKi9cbiAgZHRzSGVhZGVyPzogc3RyaW5nXG4gIC8qKlxuICAgKiBXaGV0aGVyIHRvIGRpc2FibGUgdGhlIGRlZmF1bHQgZmlsZSBoZWFkZXIgZm9yIGdlbmVyYXRlZCB0eXBlIGRlZiBmaWxlLiBPbmx5IHdvcmtzIHdoZW4gYHR5cGVkZWZgIGZlYXR1cmUgZW5hYmxlZC5cbiAgICovXG4gIG5vRHRzSGVhZGVyPzogYm9vbGVhblxuICAvKipcbiAgICogV2hldGhlciB0byBlbmFibGUgdGhlIGR0cyBjYWNoZSwgZGVmYXVsdCB0byB0cnVlXG4gICAqXG4gICAqIEBkZWZhdWx0IHRydWVcbiAgICovXG4gIGR0c0NhY2hlPzogYm9vbGVhblxuICAvKipcbiAgICogV2hldGhlciB0byBlbWl0IGFuIEVTTSBKUyBiaW5kaW5nIGZpbGUgaW5zdGVhZCBvZiBDSlMgZm9ybWF0LiBPbmx5IHdvcmtzIHdpdGggYC0tcGxhdGZvcm1gIGZsYWcuXG4gICAqL1xuICBlc20/OiBib29sZWFuXG4gIC8qKlxuICAgKiBXaGV0aGVyIHN0cmlwIHRoZSBsaWJyYXJ5IHRvIGFjaGlldmUgdGhlIG1pbmltdW0gZmlsZSBzaXplXG4gICAqL1xuICBzdHJpcD86IGJvb2xlYW5cbiAgLyoqXG4gICAqIEJ1aWxkIGluIHJlbGVhc2UgbW9kZVxuICAgKi9cbiAgcmVsZWFzZT86IGJvb2xlYW5cbiAgLyoqXG4gICAqIFZlcmJvc2VseSBsb2cgYnVpbGQgY29tbWFuZCB0cmFjZVxuICAgKi9cbiAgdmVyYm9zZT86IGJvb2xlYW5cbiAgLyoqXG4gICAqIEJ1aWxkIG9ubHkgdGhlIHNwZWNpZmllZCBiaW5hcnlcbiAgICovXG4gIGJpbj86IHN0cmluZ1xuICAvKipcbiAgICogQnVpbGQgdGhlIHNwZWNpZmllZCBsaWJyYXJ5IG9yIHRoZSBvbmUgYXQgY3dkXG4gICAqL1xuICBwYWNrYWdlPzogc3RyaW5nXG4gIC8qKlxuICAgKiBCdWlsZCBhcnRpZmFjdHMgd2l0aCB0aGUgc3BlY2lmaWVkIHByb2ZpbGVcbiAgICovXG4gIHByb2ZpbGU/OiBzdHJpbmdcbiAgLyoqXG4gICAqIFtleHBlcmltZW50YWxdIGNyb3NzLWNvbXBpbGUgZm9yIHRoZSBzcGVjaWZpZWQgdGFyZ2V0IHdpdGggYGNhcmdvLXh3aW5gIG9uIHdpbmRvd3MgYW5kIGBjYXJnby16aWdidWlsZGAgb24gb3RoZXIgcGxhdGZvcm1cbiAgICovXG4gIGNyb3NzQ29tcGlsZT86IGJvb2xlYW5cbiAgLyoqXG4gICAqIFtleHBlcmltZW50YWxdIHVzZSBbY3Jvc3NdKGh0dHBzOi8vZ2l0aHViLmNvbS9jcm9zcy1ycy9jcm9zcykgaW5zdGVhZCBvZiBgY2FyZ29gXG4gICAqL1xuICB1c2VDcm9zcz86IGJvb2xlYW5cbiAgLyoqXG4gICAqIFtleHBlcmltZW50YWxdIHVzZSBAbmFwaS1ycy9jcm9zcy10b29sY2hhaW4gdG8gY3Jvc3MtY29tcGlsZSBMaW51eCBhcm0vYXJtNjQveDY0IGdudSB0YXJnZXRzLlxuICAgKi9cbiAgdXNlTmFwaUNyb3NzPzogYm9vbGVhblxuICAvKipcbiAgICogd2F0Y2ggdGhlIGNyYXRlIGNoYW5nZXMgYW5kIGJ1aWxkIGNvbnRpbnVvdXNseSB3aXRoIGBjYXJnby13YXRjaGAgY3JhdGVzXG4gICAqL1xuICB3YXRjaD86IGJvb2xlYW5cbiAgLyoqXG4gICAqIFNwYWNlLXNlcGFyYXRlZCBsaXN0IG9mIGZlYXR1cmVzIHRvIGFjdGl2YXRlXG4gICAqL1xuICBmZWF0dXJlcz86IHN0cmluZ1tdXG4gIC8qKlxuICAgKiBBY3RpdmF0ZSBhbGwgYXZhaWxhYmxlIGZlYXR1cmVzXG4gICAqL1xuICBhbGxGZWF0dXJlcz86IGJvb2xlYW5cbiAgLyoqXG4gICAqIERvIG5vdCBhY3RpdmF0ZSB0aGUgYGRlZmF1bHRgIGZlYXR1cmVcbiAgICovXG4gIG5vRGVmYXVsdEZlYXR1cmVzPzogYm9vbGVhblxufVxuXG5leHBvcnQgZnVuY3Rpb24gYXBwbHlEZWZhdWx0QnVpbGRPcHRpb25zKG9wdGlvbnM6IEJ1aWxkT3B0aW9ucykge1xuICByZXR1cm4ge1xuICAgIGR0c0NhY2hlOiB0cnVlLFxuICAgIC4uLm9wdGlvbnMsXG4gIH1cbn1cbiIsImltcG9ydCB7IGV4ZWNTeW5jIH0gZnJvbSAnbm9kZTpjaGlsZF9wcm9jZXNzJ1xuXG5pbXBvcnQgeyBPcHRpb24gfSBmcm9tICdjbGlwYW5pb24nXG5cbmltcG9ydCB7IGJ1aWxkUHJvamVjdCB9IGZyb20gJy4uL2FwaS9idWlsZC5qcydcbmltcG9ydCB7IEJhc2VCdWlsZENvbW1hbmQgfSBmcm9tICcuLi9kZWYvYnVpbGQuanMnXG5pbXBvcnQgeyBkZWJ1Z0ZhY3RvcnkgfSBmcm9tICcuLi91dGlscy9pbmRleC5qcydcblxuY29uc3QgZGVidWcgPSBkZWJ1Z0ZhY3RvcnkoJ2J1aWxkJylcblxuZXhwb3J0IGNsYXNzIEJ1aWxkQ29tbWFuZCBleHRlbmRzIEJhc2VCdWlsZENvbW1hbmQge1xuICBwaXBlID0gT3B0aW9uLlN0cmluZygnLS1waXBlJywge1xuICAgIGRlc2NyaXB0aW9uOlxuICAgICAgJ1BpcGUgYWxsIG91dHB1dHMgZmlsZSB0byBnaXZlbiBjb21tYW5kLiBlLmcuIGBuYXBpIGJ1aWxkIC0tcGlwZSBcIm5weCBwcmV0dGllciAtLXdyaXRlXCJgJyxcbiAgfSlcblxuICBjYXJnb09wdGlvbnMgPSBPcHRpb24uUmVzdCgpXG5cbiAgYXN5bmMgZXhlY3V0ZSgpIHtcbiAgICBjb25zdCB7IHRhc2sgfSA9IGF3YWl0IGJ1aWxkUHJvamVjdCh7XG4gICAgICAuLi50aGlzLmdldE9wdGlvbnMoKSxcbiAgICAgIGNhcmdvT3B0aW9uczogdGhpcy5jYXJnb09wdGlvbnMsXG4gICAgfSlcblxuICAgIGNvbnN0IG91dHB1dHMgPSBhd2FpdCB0YXNrXG5cbiAgICBpZiAodGhpcy5waXBlKSB7XG4gICAgICBmb3IgKGNvbnN0IG91dHB1dCBvZiBvdXRwdXRzKSB7XG4gICAgICAgIGRlYnVnKCdQaXBpbmcgb3V0cHV0IGZpbGUgdG8gY29tbWFuZDogJXMnLCB0aGlzLnBpcGUpXG4gICAgICAgIHRyeSB7XG4gICAgICAgICAgZXhlY1N5bmMoYCR7dGhpcy5waXBlfSAke291dHB1dC5wYXRofWAsIHtcbiAgICAgICAgICAgIHN0ZGlvOiAnaW5oZXJpdCcsXG4gICAgICAgICAgICBjd2Q6IHRoaXMuY3dkLFxuICAgICAgICAgIH0pXG4gICAgICAgIH0gY2F0Y2ggKGUpIHtcbiAgICAgICAgICBkZWJ1Zy5lcnJvcihgRmFpbGVkIHRvIHBpcGUgb3V0cHV0IGZpbGUgJHtvdXRwdXQucGF0aH0gdG8gY29tbWFuZGApXG4gICAgICAgICAgZGVidWcuZXJyb3IoZSlcbiAgICAgICAgfVxuICAgICAgfVxuICAgIH1cbiAgfVxufVxuIiwiaW1wb3J0IHsgY3JlYXRlTnBtRGlycyB9IGZyb20gJy4uL2FwaS9jcmVhdGUtbnBtLWRpcnMuanMnXG5pbXBvcnQgeyBCYXNlQ3JlYXRlTnBtRGlyc0NvbW1hbmQgfSBmcm9tICcuLi9kZWYvY3JlYXRlLW5wbS1kaXJzLmpzJ1xuXG5leHBvcnQgY2xhc3MgQ3JlYXRlTnBtRGlyc0NvbW1hbmQgZXh0ZW5kcyBCYXNlQ3JlYXRlTnBtRGlyc0NvbW1hbmQge1xuICBhc3luYyBleGVjdXRlKCkge1xuICAgIGF3YWl0IGNyZWF0ZU5wbURpcnModGhpcy5nZXRPcHRpb25zKCkpXG4gIH1cbn1cbiIsImltcG9ydCB7IENvbW1hbmQgfSBmcm9tICdjbGlwYW5pb24nXG5cbi8qKlxuICogQSBjb21tYW5kIHRoYXQgcHJpbnRzIHRoZSB1c2FnZSBvZiBhbGwgY29tbWFuZHMuXG4gKlxuICogUGF0aHM6IGAtaGAsIGAtLWhlbHBgXG4gKi9cbmV4cG9ydCBjbGFzcyBIZWxwQ29tbWFuZCBleHRlbmRzIENvbW1hbmQ8YW55PiB7XG4gIHN0YXRpYyBwYXRocyA9IFtbYC1oYF0sIFtgLS1oZWxwYF1dXG4gIGFzeW5jIGV4ZWN1dGUoKSB7XG4gICAgYXdhaXQgdGhpcy5jb250ZXh0LnN0ZG91dC53cml0ZSh0aGlzLmNsaS51c2FnZSgpKVxuICB9XG59XG4iLCJpbXBvcnQgcGF0aCBmcm9tICdub2RlOnBhdGgnXG5cbmltcG9ydCB7IGlucHV0LCBzZWxlY3QsIGNoZWNrYm94LCBjb25maXJtIH0gZnJvbSAnQGlucXVpcmVyL3Byb21wdHMnXG5pbXBvcnQgeyBPcHRpb24gfSBmcm9tICdjbGlwYW5pb24nXG5cbmltcG9ydCB7IG5ld1Byb2plY3QgfSBmcm9tICcuLi9hcGkvbmV3LmpzJ1xuaW1wb3J0IHsgQmFzZU5ld0NvbW1hbmQgfSBmcm9tICcuLi9kZWYvbmV3LmpzJ1xuaW1wb3J0IHtcbiAgQVZBSUxBQkxFX1RBUkdFVFMsXG4gIGRlYnVnRmFjdG9yeSxcbiAgREVGQVVMVF9UQVJHRVRTLFxuICB0eXBlIFRhcmdldFRyaXBsZSxcbn0gZnJvbSAnLi4vdXRpbHMvaW5kZXguanMnXG5pbXBvcnQgeyBuYXBpRW5naW5lUmVxdWlyZW1lbnQgfSBmcm9tICcuLi91dGlscy92ZXJzaW9uLmpzJ1xuXG5jb25zdCBkZWJ1ZyA9IGRlYnVnRmFjdG9yeSgnbmV3JylcblxuZXhwb3J0IGNsYXNzIE5ld0NvbW1hbmQgZXh0ZW5kcyBCYXNlTmV3Q29tbWFuZCB7XG4gIGludGVyYWN0aXZlID0gT3B0aW9uLkJvb2xlYW4oJy0taW50ZXJhY3RpdmUsLWknLCB0cnVlLCB7XG4gICAgZGVzY3JpcHRpb246XG4gICAgICAnQXNrIHByb2plY3QgYmFzaWMgaW5mb3JtYXRpb24gaW50ZXJhY3RpdmVseSB3aXRob3V0IGp1c3QgdXNpbmcgdGhlIGRlZmF1bHQuJyxcbiAgfSlcblxuICBhc3luYyBleGVjdXRlKCkge1xuICAgIHRyeSB7XG4gICAgICBjb25zdCBvcHRpb25zID0gYXdhaXQgdGhpcy5mZXRjaE9wdGlvbnMoKVxuICAgICAgYXdhaXQgbmV3UHJvamVjdChvcHRpb25zKVxuICAgICAgcmV0dXJuIDBcbiAgICB9IGNhdGNoIChlKSB7XG4gICAgICBkZWJ1ZygnRmFpbGVkIHRvIGNyZWF0ZSBuZXcgcHJvamVjdCcpXG4gICAgICBkZWJ1Zy5lcnJvcihlKVxuICAgICAgcmV0dXJuIDFcbiAgICB9XG4gIH1cblxuICBwcml2YXRlIGFzeW5jIGZldGNoT3B0aW9ucygpIHtcbiAgICBjb25zdCBjbWRPcHRpb25zID0gc3VwZXIuZ2V0T3B0aW9ucygpXG5cbiAgICBpZiAodGhpcy5pbnRlcmFjdGl2ZSkge1xuICAgICAgY29uc3QgdGFyZ2V0UGF0aDogc3RyaW5nID0gY21kT3B0aW9ucy5wYXRoXG4gICAgICAgID8gY21kT3B0aW9ucy5wYXRoXG4gICAgICAgIDogYXdhaXQgaW5xdWlyZXJQcm9qZWN0UGF0aCgpXG4gICAgICBjbWRPcHRpb25zLnBhdGggPSB0YXJnZXRQYXRoXG4gICAgICByZXR1cm4ge1xuICAgICAgICAuLi5jbWRPcHRpb25zLFxuICAgICAgICBuYW1lOiBhd2FpdCB0aGlzLmZldGNoTmFtZShwYXRoLnBhcnNlKHRhcmdldFBhdGgpLmJhc2UpLFxuICAgICAgICBtaW5Ob2RlQXBpVmVyc2lvbjogYXdhaXQgdGhpcy5mZXRjaE5hcGlWZXJzaW9uKCksXG4gICAgICAgIHRhcmdldHM6IGF3YWl0IHRoaXMuZmV0Y2hUYXJnZXRzKCksXG4gICAgICAgIGxpY2Vuc2U6IGF3YWl0IHRoaXMuZmV0Y2hMaWNlbnNlKCksXG4gICAgICAgIGVuYWJsZVR5cGVEZWY6IGF3YWl0IHRoaXMuZmV0Y2hUeXBlRGVmKCksXG4gICAgICAgIGVuYWJsZUdpdGh1YkFjdGlvbnM6IGF3YWl0IHRoaXMuZmV0Y2hHaXRodWJBY3Rpb25zKCksXG4gICAgICB9XG4gICAgfVxuXG4gICAgcmV0dXJuIGNtZE9wdGlvbnNcbiAgfVxuXG4gIHByaXZhdGUgYXN5bmMgZmV0Y2hOYW1lKGRlZmF1bHROYW1lOiBzdHJpbmcpOiBQcm9taXNlPHN0cmluZz4ge1xuICAgIHJldHVybiAoXG4gICAgICB0aGlzLiQkbmFtZSA/P1xuICAgICAgaW5wdXQoe1xuICAgICAgICBtZXNzYWdlOiAnUGFja2FnZSBuYW1lICh0aGUgbmFtZSBmaWVsZCBpbiB5b3VyIHBhY2thZ2UuanNvbiBmaWxlKScsXG4gICAgICAgIGRlZmF1bHQ6IGRlZmF1bHROYW1lLFxuICAgICAgfSlcbiAgICApXG4gIH1cblxuICBwcml2YXRlIGFzeW5jIGZldGNoTGljZW5zZSgpOiBQcm9taXNlPHN0cmluZz4ge1xuICAgIHJldHVybiBpbnB1dCh7XG4gICAgICBtZXNzYWdlOiAnTGljZW5zZSBmb3Igb3Blbi1zb3VyY2VkIHByb2plY3QnLFxuICAgICAgZGVmYXVsdDogdGhpcy5saWNlbnNlLFxuICAgIH0pXG4gIH1cblxuICBwcml2YXRlIGFzeW5jIGZldGNoTmFwaVZlcnNpb24oKTogUHJvbWlzZTxudW1iZXI+IHtcbiAgICByZXR1cm4gc2VsZWN0KHtcbiAgICAgIG1lc3NhZ2U6ICdNaW5pbXVtIG5vZGUtYXBpIHZlcnNpb24gKHdpdGggbm9kZSB2ZXJzaW9uIHJlcXVpcmVtZW50KScsXG4gICAgICBsb29wOiBmYWxzZSxcbiAgICAgIHBhZ2VTaXplOiAxMCxcbiAgICAgIGNob2ljZXM6IEFycmF5LmZyb20oeyBsZW5ndGg6IDggfSwgKF8sIGkpID0+ICh7XG4gICAgICAgIG5hbWU6IGBuYXBpJHtpICsgMX0gKCR7bmFwaUVuZ2luZVJlcXVpcmVtZW50KGkgKyAxKX0pYCxcbiAgICAgICAgdmFsdWU6IGkgKyAxLFxuICAgICAgfSkpLFxuICAgICAgLy8gY2hvaWNlIGluZGV4XG4gICAgICBkZWZhdWx0OiB0aGlzLm1pbk5vZGVBcGlWZXJzaW9uIC0gMSxcbiAgICB9KVxuICB9XG5cbiAgcHJpdmF0ZSBhc3luYyBmZXRjaFRhcmdldHMoKTogUHJvbWlzZTxUYXJnZXRUcmlwbGVbXT4ge1xuICAgIGlmICh0aGlzLmVuYWJsZUFsbFRhcmdldHMpIHtcbiAgICAgIHJldHVybiBBVkFJTEFCTEVfVEFSR0VUUy5jb25jYXQoKVxuICAgIH1cblxuICAgIGNvbnN0IHRhcmdldHMgPSBhd2FpdCBjaGVja2JveCh7XG4gICAgICBsb29wOiBmYWxzZSxcbiAgICAgIG1lc3NhZ2U6ICdDaG9vc2UgdGFyZ2V0KHMpIHlvdXIgY3JhdGUgd2lsbCBiZSBjb21waWxlZCB0bycsXG4gICAgICBjaG9pY2VzOiBBVkFJTEFCTEVfVEFSR0VUUy5tYXAoKHRhcmdldCkgPT4gKHtcbiAgICAgICAgbmFtZTogdGFyZ2V0LFxuICAgICAgICB2YWx1ZTogdGFyZ2V0LFxuICAgICAgICAvLyBAdHMtZXhwZWN0LWVycm9yXG4gICAgICAgIGNoZWNrZWQ6IERFRkFVTFRfVEFSR0VUUy5pbmNsdWRlcyh0YXJnZXQpLFxuICAgICAgfSkpLFxuICAgIH0pXG5cbiAgICByZXR1cm4gdGFyZ2V0c1xuICB9XG5cbiAgcHJpdmF0ZSBhc3luYyBmZXRjaFR5cGVEZWYoKTogUHJvbWlzZTxib29sZWFuPiB7XG4gICAgY29uc3QgZW5hYmxlVHlwZURlZiA9IGF3YWl0IGNvbmZpcm0oe1xuICAgICAgbWVzc2FnZTogJ0VuYWJsZSB0eXBlIGRlZmluaXRpb24gYXV0by1nZW5lcmF0aW9uJyxcbiAgICAgIGRlZmF1bHQ6IHRoaXMuZW5hYmxlVHlwZURlZixcbiAgICB9KVxuXG4gICAgcmV0dXJuIGVuYWJsZVR5cGVEZWZcbiAgfVxuXG4gIHByaXZhdGUgYXN5bmMgZmV0Y2hHaXRodWJBY3Rpb25zKCk6IFByb21pc2U8Ym9vbGVhbj4ge1xuICAgIGNvbnN0IGVuYWJsZUdpdGh1YkFjdGlvbnMgPSBhd2FpdCBjb25maXJtKHtcbiAgICAgIG1lc3NhZ2U6ICdFbmFibGUgR2l0aHViIEFjdGlvbnMgQ0knLFxuICAgICAgZGVmYXVsdDogdGhpcy5lbmFibGVHaXRodWJBY3Rpb25zLFxuICAgIH0pXG5cbiAgICByZXR1cm4gZW5hYmxlR2l0aHViQWN0aW9uc1xuICB9XG59XG5cbmFzeW5jIGZ1bmN0aW9uIGlucXVpcmVyUHJvamVjdFBhdGgoKTogUHJvbWlzZTxzdHJpbmc+IHtcbiAgcmV0dXJuIGlucHV0KHtcbiAgICBtZXNzYWdlOiAnVGFyZ2V0IHBhdGggdG8gY3JlYXRlIHRoZSBwcm9qZWN0LCByZWxhdGl2ZSB0byBjd2QuJyxcbiAgfSkudGhlbigocGF0aCkgPT4ge1xuICAgIGlmICghcGF0aCkge1xuICAgICAgcmV0dXJuIGlucXVpcmVyUHJvamVjdFBhdGgoKVxuICAgIH1cbiAgICByZXR1cm4gcGF0aFxuICB9KVxufVxuIiwiaW1wb3J0IHsgcHJlUHVibGlzaCB9IGZyb20gJy4uL2FwaS9wcmUtcHVibGlzaC5qcydcbmltcG9ydCB7IEJhc2VQcmVQdWJsaXNoQ29tbWFuZCB9IGZyb20gJy4uL2RlZi9wcmUtcHVibGlzaC5qcydcblxuZXhwb3J0IGNsYXNzIFByZVB1Ymxpc2hDb21tYW5kIGV4dGVuZHMgQmFzZVByZVB1Ymxpc2hDb21tYW5kIHtcbiAgYXN5bmMgZXhlY3V0ZSgpIHtcbiAgICAvLyBAdHMtZXhwZWN0LWVycm9yIGNvbnN0ICducG0nIHwgJ2xlcm5hJyB0byBzdHJpbmdcbiAgICBhd2FpdCBwcmVQdWJsaXNoKHRoaXMuZ2V0T3B0aW9ucygpKVxuICB9XG59XG4iLCJpbXBvcnQgeyBpbnB1dCB9IGZyb20gJ0BpbnF1aXJlci9wcm9tcHRzJ1xuXG5pbXBvcnQgeyByZW5hbWVQcm9qZWN0IH0gZnJvbSAnLi4vYXBpL3JlbmFtZS5qcydcbmltcG9ydCB7IEJhc2VSZW5hbWVDb21tYW5kIH0gZnJvbSAnLi4vZGVmL3JlbmFtZS5qcydcblxuZXhwb3J0IGNsYXNzIFJlbmFtZUNvbW1hbmQgZXh0ZW5kcyBCYXNlUmVuYW1lQ29tbWFuZCB7XG4gIGFzeW5jIGV4ZWN1dGUoKSB7XG4gICAgY29uc3Qgb3B0aW9ucyA9IHRoaXMuZ2V0T3B0aW9ucygpXG4gICAgaWYgKCFvcHRpb25zLm5hbWUpIHtcbiAgICAgIGNvbnN0IG5hbWUgPSBhd2FpdCBpbnB1dCh7XG4gICAgICAgIG1lc3NhZ2U6IGBFbnRlciB0aGUgbmV3IHBhY2thZ2UgbmFtZSBpbiB0aGUgcGFja2FnZS5qc29uYCxcbiAgICAgICAgcmVxdWlyZWQ6IHRydWUsXG4gICAgICB9KVxuICAgICAgb3B0aW9ucy5uYW1lID0gbmFtZVxuICAgIH1cbiAgICBpZiAoIW9wdGlvbnMuYmluYXJ5TmFtZSkge1xuICAgICAgY29uc3QgYmluYXJ5TmFtZSA9IGF3YWl0IGlucHV0KHtcbiAgICAgICAgbWVzc2FnZTogYEVudGVyIHRoZSBuZXcgYmluYXJ5IG5hbWVgLFxuICAgICAgICByZXF1aXJlZDogdHJ1ZSxcbiAgICAgIH0pXG4gICAgICBvcHRpb25zLmJpbmFyeU5hbWUgPSBiaW5hcnlOYW1lXG4gICAgfVxuICAgIGF3YWl0IHJlbmFtZVByb2plY3Qob3B0aW9ucylcbiAgfVxufVxuIiwiaW1wb3J0IHsgdW5pdmVyc2FsaXplQmluYXJpZXMgfSBmcm9tICcuLi9hcGkvdW5pdmVyc2FsaXplLmpzJ1xuaW1wb3J0IHsgQmFzZVVuaXZlcnNhbGl6ZUNvbW1hbmQgfSBmcm9tICcuLi9kZWYvdW5pdmVyc2FsaXplLmpzJ1xuXG5leHBvcnQgY2xhc3MgVW5pdmVyc2FsaXplQ29tbWFuZCBleHRlbmRzIEJhc2VVbml2ZXJzYWxpemVDb21tYW5kIHtcbiAgYXN5bmMgZXhlY3V0ZSgpIHtcbiAgICBhd2FpdCB1bml2ZXJzYWxpemVCaW5hcmllcyh0aGlzLmdldE9wdGlvbnMoKSlcbiAgfVxufVxuIiwiaW1wb3J0IHsgdmVyc2lvbiB9IGZyb20gJy4uL2FwaS92ZXJzaW9uLmpzJ1xuaW1wb3J0IHsgQmFzZVZlcnNpb25Db21tYW5kIH0gZnJvbSAnLi4vZGVmL3ZlcnNpb24uanMnXG5cbmV4cG9ydCBjbGFzcyBWZXJzaW9uQ29tbWFuZCBleHRlbmRzIEJhc2VWZXJzaW9uQ29tbWFuZCB7XG4gIGFzeW5jIGV4ZWN1dGUoKSB7XG4gICAgYXdhaXQgdmVyc2lvbih0aGlzLmdldE9wdGlvbnMoKSlcbiAgfVxufVxuIiwiaW1wb3J0IHsgQ2xpIH0gZnJvbSAnY2xpcGFuaW9uJ1xuXG5pbXBvcnQgeyBjb2xsZWN0QXJ0aWZhY3RzIH0gZnJvbSAnLi9hcGkvYXJ0aWZhY3RzLmpzJ1xuaW1wb3J0IHsgYnVpbGRQcm9qZWN0IH0gZnJvbSAnLi9hcGkvYnVpbGQuanMnXG5pbXBvcnQgeyBjcmVhdGVOcG1EaXJzIH0gZnJvbSAnLi9hcGkvY3JlYXRlLW5wbS1kaXJzLmpzJ1xuaW1wb3J0IHsgbmV3UHJvamVjdCB9IGZyb20gJy4vYXBpL25ldy5qcydcbmltcG9ydCB7IHByZVB1Ymxpc2ggfSBmcm9tICcuL2FwaS9wcmUtcHVibGlzaC5qcydcbmltcG9ydCB7IHJlbmFtZVByb2plY3QgfSBmcm9tICcuL2FwaS9yZW5hbWUuanMnXG5pbXBvcnQgeyB1bml2ZXJzYWxpemVCaW5hcmllcyB9IGZyb20gJy4vYXBpL3VuaXZlcnNhbGl6ZS5qcydcbmltcG9ydCB7IHZlcnNpb24gfSBmcm9tICcuL2FwaS92ZXJzaW9uLmpzJ1xuaW1wb3J0IHsgQXJ0aWZhY3RzQ29tbWFuZCB9IGZyb20gJy4vY29tbWFuZHMvYXJ0aWZhY3RzLmpzJ1xuaW1wb3J0IHsgQnVpbGRDb21tYW5kIH0gZnJvbSAnLi9jb21tYW5kcy9idWlsZC5qcydcbmltcG9ydCB7IENyZWF0ZU5wbURpcnNDb21tYW5kIH0gZnJvbSAnLi9jb21tYW5kcy9jcmVhdGUtbnBtLWRpcnMuanMnXG5pbXBvcnQgeyBIZWxwQ29tbWFuZCB9IGZyb20gJy4vY29tbWFuZHMvaGVscC5qcydcbmltcG9ydCB7IE5ld0NvbW1hbmQgfSBmcm9tICcuL2NvbW1hbmRzL25ldy5qcydcbmltcG9ydCB7IFByZVB1Ymxpc2hDb21tYW5kIH0gZnJvbSAnLi9jb21tYW5kcy9wcmUtcHVibGlzaC5qcydcbmltcG9ydCB7IFJlbmFtZUNvbW1hbmQgfSBmcm9tICcuL2NvbW1hbmRzL3JlbmFtZS5qcydcbmltcG9ydCB7IFVuaXZlcnNhbGl6ZUNvbW1hbmQgfSBmcm9tICcuL2NvbW1hbmRzL3VuaXZlcnNhbGl6ZS5qcydcbmltcG9ydCB7IFZlcnNpb25Db21tYW5kIH0gZnJvbSAnLi9jb21tYW5kcy92ZXJzaW9uLmpzJ1xuaW1wb3J0IHsgQ0xJX1ZFUlNJT04gfSBmcm9tICcuL3V0aWxzL21pc2MuanMnXG5cbmV4cG9ydCBjb25zdCBjbGkgPSBuZXcgQ2xpKHtcbiAgYmluYXJ5TmFtZTogJ25hcGknLFxuICBiaW5hcnlWZXJzaW9uOiBDTElfVkVSU0lPTixcbn0pXG5cbmNsaS5yZWdpc3RlcihOZXdDb21tYW5kKVxuY2xpLnJlZ2lzdGVyKEJ1aWxkQ29tbWFuZClcbmNsaS5yZWdpc3RlcihDcmVhdGVOcG1EaXJzQ29tbWFuZClcbmNsaS5yZWdpc3RlcihBcnRpZmFjdHNDb21tYW5kKVxuY2xpLnJlZ2lzdGVyKFVuaXZlcnNhbGl6ZUNvbW1hbmQpXG5jbGkucmVnaXN0ZXIoUmVuYW1lQ29tbWFuZClcbmNsaS5yZWdpc3RlcihQcmVQdWJsaXNoQ29tbWFuZClcbmNsaS5yZWdpc3RlcihWZXJzaW9uQ29tbWFuZClcbmNsaS5yZWdpc3RlcihIZWxwQ29tbWFuZClcblxuLyoqXG4gKlxuICogQHVzYWdlXG4gKlxuICogYGBgdHNcbiAqIGNvbnN0IGNsaSA9IG5ldyBOYXBpQ2xpKClcbiAqXG4gKiBjbGkuYnVpbGQoe1xuICogICBjd2Q6ICcvcGF0aC90by95b3VyL3Byb2plY3QnLFxuICogfSlcbiAqIGBgYFxuICovXG5leHBvcnQgY2xhc3MgTmFwaUNsaSB7XG4gIGFydGlmYWN0cyA9IGNvbGxlY3RBcnRpZmFjdHNcbiAgbmV3ID0gbmV3UHJvamVjdFxuICBidWlsZCA9IGJ1aWxkUHJvamVjdFxuICBjcmVhdGVOcG1EaXJzID0gY3JlYXRlTnBtRGlyc1xuICBwcmVQdWJsaXNoID0gcHJlUHVibGlzaFxuICByZW5hbWUgPSByZW5hbWVQcm9qZWN0XG4gIHVuaXZlcnNhbGl6ZSA9IHVuaXZlcnNhbGl6ZUJpbmFyaWVzXG4gIHZlcnNpb24gPSB2ZXJzaW9uXG59XG5cbmV4cG9ydCBmdW5jdGlvbiBjcmVhdGVCdWlsZENvbW1hbmQoYXJnczogc3RyaW5nW10pOiBCdWlsZENvbW1hbmQge1xuICByZXR1cm4gY2xpLnByb2Nlc3MoWydidWlsZCcsIC4uLmFyZ3NdKSBhcyBCdWlsZENvbW1hbmRcbn1cblxuZXhwb3J0IGZ1bmN0aW9uIGNyZWF0ZUFydGlmYWN0c0NvbW1hbmQoYXJnczogc3RyaW5nW10pOiBBcnRpZmFjdHNDb21tYW5kIHtcbiAgcmV0dXJuIGNsaS5wcm9jZXNzKFsnYXJ0aWZhY3RzJywgLi4uYXJnc10pIGFzIEFydGlmYWN0c0NvbW1hbmRcbn1cblxuZXhwb3J0IGZ1bmN0aW9uIGNyZWF0ZUNyZWF0ZU5wbURpcnNDb21tYW5kKFxuICBhcmdzOiBzdHJpbmdbXSxcbik6IENyZWF0ZU5wbURpcnNDb21tYW5kIHtcbiAgcmV0dXJuIGNsaS5wcm9jZXNzKFsnY3JlYXRlLW5wbS1kaXJzJywgLi4uYXJnc10pIGFzIENyZWF0ZU5wbURpcnNDb21tYW5kXG59XG5cbmV4cG9ydCBmdW5jdGlvbiBjcmVhdGVQcmVQdWJsaXNoQ29tbWFuZChhcmdzOiBzdHJpbmdbXSk6IFByZVB1Ymxpc2hDb21tYW5kIHtcbiAgcmV0dXJuIGNsaS5wcm9jZXNzKFsncHJlLXB1Ymxpc2gnLCAuLi5hcmdzXSkgYXMgUHJlUHVibGlzaENvbW1hbmRcbn1cblxuZXhwb3J0IGZ1bmN0aW9uIGNyZWF0ZVJlbmFtZUNvbW1hbmQoYXJnczogc3RyaW5nW10pOiBSZW5hbWVDb21tYW5kIHtcbiAgcmV0dXJuIGNsaS5wcm9jZXNzKFsncmVuYW1lJywgLi4uYXJnc10pIGFzIFJlbmFtZUNvbW1hbmRcbn1cblxuZXhwb3J0IGZ1bmN0aW9uIGNyZWF0ZVVuaXZlcnNhbGl6ZUNvbW1hbmQoYXJnczogc3RyaW5nW10pOiBVbml2ZXJzYWxpemVDb21tYW5kIHtcbiAgcmV0dXJuIGNsaS5wcm9jZXNzKFsndW5pdmVyc2FsaXplJywgLi4uYXJnc10pIGFzIFVuaXZlcnNhbGl6ZUNvbW1hbmRcbn1cblxuZXhwb3J0IGZ1bmN0aW9uIGNyZWF0ZVZlcnNpb25Db21tYW5kKGFyZ3M6IHN0cmluZ1tdKTogVmVyc2lvbkNvbW1hbmQge1xuICByZXR1cm4gY2xpLnByb2Nlc3MoWyd2ZXJzaW9uJywgLi4uYXJnc10pIGFzIFZlcnNpb25Db21tYW5kXG59XG5cbmV4cG9ydCBmdW5jdGlvbiBjcmVhdGVOZXdDb21tYW5kKGFyZ3M6IHN0cmluZ1tdKTogTmV3Q29tbWFuZCB7XG4gIHJldHVybiBjbGkucHJvY2VzcyhbJ25ldycsIC4uLmFyZ3NdKSBhcyBOZXdDb21tYW5kXG59XG5cbmV4cG9ydCB7IHBhcnNlVHJpcGxlIH0gZnJvbSAnLi91dGlscy90YXJnZXQuanMnXG5leHBvcnQge1xuICB0eXBlIEdlbmVyYXRlVHlwZURlZk9wdGlvbnMsXG4gIHR5cGUgV3JpdGVKc0JpbmRpbmdPcHRpb25zLFxuICB3cml0ZUpzQmluZGluZyxcbiAgZ2VuZXJhdGVUeXBlRGVmLFxufSBmcm9tICcuL2FwaS9idWlsZC5qcydcbmV4cG9ydCB7IHJlYWROYXBpQ29uZmlnIH0gZnJvbSAnLi91dGlscy9jb25maWcuanMnXG4iLCIjIS91c3IvYmluL2VudiBub2RlXG5cbmltcG9ydCB7IGNsaSB9IGZyb20gJy4vaW5kZXguanMnXG5cbnZvaWQgY2xpLnJ1bkV4aXQocHJvY2Vzcy5hcmd2LnNsaWNlKDIpKVxuIl0sInhfZ29vZ2xlX2lnbm9yZUxpc3QiOlsxOSwyMCwyMSwyMiwyMywyNCwyNSwyNl0sIm1hcHBpbmdzIjoiOzs7Ozs7Ozs7Ozs7Ozs7Ozs7OztBQUlBLElBQXNCLHVCQUF0QixjQUFtRCxRQUFRO0NBQ3pELE9BQU8sUUFBUSxDQUFDLENBQUMsWUFBWSxDQUFDO0NBRTlCLE9BQU8sUUFBUSxRQUFRLE1BQU0sRUFDM0IsYUFDRSw2RUFDSCxDQUFDO0NBRUYsTUFBTSxPQUFPLE9BQU8sU0FBUyxRQUFRLEtBQUssRUFBRSxFQUMxQyxhQUNFLHNIQUNILENBQUM7Q0FFRixhQUFzQixPQUFPLE9BQU8sb0JBQW9CLEVBQ3RELGFBQWEsbUNBQ2QsQ0FBQztDQUVGLGtCQUFrQixPQUFPLE9BQU8sdUJBQXVCLGdCQUFnQixFQUNyRSxhQUFhLDBCQUNkLENBQUM7Q0FFRixZQUFZLE9BQU8sT0FBTyxzQkFBc0IsZUFBZSxFQUM3RCxhQUNFLGlHQUNILENBQUM7Q0FFRixTQUFTLE9BQU8sT0FBTyxhQUFhLE9BQU8sRUFDekMsYUFBYSxpREFDZCxDQUFDO0NBRUYsaUJBQTBCLE9BQU8sT0FBTyxzQkFBc0IsRUFDNUQsYUFDRSxtRkFDSCxDQUFDO0NBRUYsYUFB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