import * as clipanion1 from "clipanion";
import { Cli, Command } from "clipanion";

//#region src/def/artifacts.d.ts
declare abstract class BaseArtifactsCommand extends Command {
  static paths: string[][];
  static usage: clipanion1.Usage;
  cwd: string;
  configPath?: string;
  packageJsonPath: string;
  outputDir: string;
  npmDir: string;
  buildOutputDir?: string;
  getOptions(): {
    cwd: string;
    configPath: string | undefined;
    packageJsonPath: string;
    outputDir: string;
    npmDir: string;
    buildOutputDir: string | undefined;
  };
}
/**
 * Copy artifacts from Github Actions into npm packages and ready to publish
 */
interface ArtifactsOptions {
  /**
   * The working directory of where napi command will be executed in, all other paths options are relative to this path
   *
   * @default process.cwd()
   */
  cwd?: string;
  /**
   * Path to `napi` config json file
   */
  configPath?: string;
  /**
   * Path to `package.json`
   *
   * @default 'package.json'
   */
  packageJsonPath?: string;
  /**
   * Path to the folder where all built `.node` files put, same as `--output-dir` of build command
   *
   * @default './artifacts'
   */
  outputDir?: string;
  /**
   * Path to the folder where the npm packages put
   *
   * @default 'npm'
   */
  npmDir?: string;
  /**
   * Path to the build output dir, only needed when targets contains `wasm32-wasi-*`
   */
  buildOutputDir?: string;
}
//#endregion
//#region src/api/artifacts.d.ts
declare function collectArtifacts(userOptions: ArtifactsOptions): Promise<void>;
//#endregion
//#region src/def/build.d.ts
declare abstract class BaseBuildCommand extends Command {
  static paths: string[][];
  static usage: clipanion1.Usage;
  target?: string;
  cwd?: string;
  manifestPath?: string;
  configPath?: string;
  packageJsonPath?: string;
  targetDir?: string;
  outputDir?: string;
  platform?: boolean;
  jsPackageName?: string;
  constEnum?: boolean;
  jsBinding?: string;
  noJsBinding?: boolean;
  dts?: string;
  dtsHeader?: string;
  noDtsHeader?: boolean;
  dtsCache: boolean;
  esm?: boolean;
  strip?: boolean;
  release?: boolean;
  verbose?: boolean;
  bin?: string;
  package?: string;
  profile?: string;
  crossCompile?: boolean;
  useCross?: boolean;
  useNapiCross?: boolean;
  watch?: boolean;
  features?: string[];
  allFeatures?: boolean;
  noDefaultFeatures?: boolean;
  getOptions(): {
    target: string | undefined;
    cwd: string | undefined;
    manifestPath: string | undefined;
    configPath: string | undefined;
    packageJsonPath: string | undefined;
    targetDir: string | undefined;
    outputDir: string | undefined;
    platform: boolean | undefined;
    jsPackageName: string | undefined;
    constEnum: boolean | undefined;
    jsBinding: string | undefined;
    noJsBinding: boolean | undefined;
    dts: string | undefined;
    dtsHeader: string | undefined;
    noDtsHeader: boolean | undefined;
    dtsCache: boolean;
    esm: boolean | undefined;
    strip: boolean | undefined;
    release: boolean | undefined;
    verbose: boolean | undefined;
    bin: string | undefined;
    package: string | undefined;
    profile: string | undefined;
    crossCompile: boolean | undefined;
    useCross: boolean | undefined;
    useNapiCross: boolean | undefined;
    watch: boolean | undefined;
    features: string[] | undefined;
    allFeatures: boolean | undefined;
    noDefaultFeatures: boolean | undefined;
  };
}
/**
 * Build the NAPI-RS project
 */
interface BuildOptions {
  /**
   * Build for the target triple, bypassed to `cargo build --target`
   */
  target?: string;
  /**
   * The working directory of where napi command will be executed in, all other paths options are relative to this path
   */
  cwd?: string;
  /**
   * Path to `Cargo.toml`
   */
  manifestPath?: string;
  /**
   * Path to `napi` config json file
   */
  configPath?: string;
  /**
   * Path to `package.json`
   */
  packageJsonPath?: string;
  /**
   * Directory for all crate generated artifacts, see `cargo build --target-dir`
   */
  targetDir?: string;
  /**
   * Path to where all the built files would be put. Default to the crate folder
   */
  outputDir?: string;
  /**
   * Add platform triple to the generated nodejs binding file, eg: `[name].linux-x64-gnu.node`
   */
  platform?: boolean;
  /**
   * Package name in generated js binding file. Only works with `--platform` flag
   */
  jsPackageName?: string;
  /**
   * Whether generate const enum for typescript bindings
   */
  constEnum?: boolean;
  /**
   * Path and filename of generated JS binding file. Only works with `--platform` flag. Relative to `--output-dir`.
   */
  jsBinding?: string;
  /**
   * Whether to disable the generation JS binding file. Only works with `--platform` flag.
   */
  noJsBinding?: boolean;
  /**
   * Path and filename of generated type def file. Relative to `--output-dir`
   */
  dts?: string;
  /**
   * Custom file header for generated type def file. Only works when `typedef` feature enabled.
   */
  dtsHeader?: string;
  /**
   * Whether to disable the default file header for generated type def file. Only works when `typedef` feature enabled.
   */
  noDtsHeader?: boolean;
  /**
   * Whether to enable the dts cache, default to true
   *
   * @default true
   */
  dtsCache?: boolean;
  /**
   * Whether to emit an ESM JS binding file instead of CJS format. Only works with `--platform` flag.
   */
  esm?: boolean;
  /**
   * Whether strip the library to achieve the minimum file size
   */
  strip?: boolean;
  /**
   * Build in release mode
   */
  release?: boolean;
  /**
   * Verbosely log build command trace
   */
  verbose?: boolean;
  /**
   * Build only the specified binary
   */
  bin?: string;
  /**
   * Build the specified library or the one at cwd
   */
  package?: string;
  /**
   * Build artifacts with the specified profile
   */
  profile?: string;
  /**
   * [experimental] cross-compile for the specified target with `cargo-xwin` on windows and `cargo-zigbuild` on other platform
   */
  crossCompile?: boolean;
  /**
   * [experimental] use [cross](https://github.com/cross-rs/cross) instead of `cargo`
   */
  useCross?: boolean;
  /**
   * [experimental] use @napi-rs/cross-toolchain to cross-compile Linux arm/arm64/x64 gnu targets.
   */
  useNapiCross?: boolean;
  /**
   * watch the crate changes and build continuously with `cargo-watch` crates
   */
  watch?: boolean;
  /**
   * Space-separated list of features to activate
   */
  features?: string[];
  /**
   * Activate all available features
   */
  allFeatures?: boolean;
  /**
   * Do not activate the `default` feature
   */
  noDefaultFeatures?: boolean;
}
//#endregion
//#region src/api/build.d.ts
type OutputKind = 'js' | 'dts' | 'node' | 'exe' | 'wasm';
type Output = {
  kind: OutputKind;
  path: string;
};
type BuildOptions$1 = BuildOptions & {
  cargoOptions?: string[];
};
declare function buildProject(rawOptions: BuildOptions$1): Promise<{
  task: Promise<Output[]>;
  abort: () => void;
}>;
interface WriteJsBindingOptions {
  platform?: boolean;
  noJsBinding?: boolean;
  idents: string[];
  jsBinding?: string;
  esm?: boolean;
  binaryName: string;
  packageName: string;
  version: string;
  outputDir: string;
}
declare function writeJsBinding(options: WriteJsBindingOptions): Promise<Output | undefined>;
interface GenerateTypeDefOptions {
  typeDefDir: string;
  noDtsHeader?: boolean;
  dtsHeader?: string;
  dtsHeaderFile?: string;
  configDtsHeader?: string;
  configDtsHeaderFile?: string;
  constEnum?: boolean;
  cwd: string;
}
declare function generateTypeDef(options: GenerateTypeDefOptions): Promise<{
  exports: string[];
  dts: string;
}>;
//#endregion
//#region src/def/create-npm-dirs.d.ts
declare abstract class BaseCreateNpmDirsCommand extends Command {
  static paths: string[][];
  static usage: clipanion1.Usage;
  cwd: string;
  configPath?: string;
  packageJsonPath: string;
  npmDir: string;
  dryRun: boolean;
  getOptions(): {
    cwd: string;
    configPath: string | undefined;
    packageJsonPath: string;
    npmDir: string;
    dryRun: boolean;
  };
}
/**
 * Create npm package dirs for different platforms
 */
interface CreateNpmDirsOptions {
  /**
   * The working directory of where napi command will be executed in, all other paths options are relative to this path
   *
   * @default process.cwd()
   */
  cwd?: string;
  /**
   * Path to `napi` config json file
   */
  configPath?: string;
  /**
   * Path to `package.json`
   *
   * @default 'package.json'
   */
  packageJsonPath?: string;
  /**
   * Path to the folder where the npm packages put
   *
   * @default 'npm'
   */
  npmDir?: string;
  /**
   * Dry run without touching file system
   *
   * @default false
   */
  dryRun?: boolean;
}
//#endregion
//#region src/api/create-npm-dirs.d.ts
declare function createNpmDirs(userOptions: CreateNpmDirsOptions): Promise<void>;
//#endregion
//#region src/def/new.d.ts
declare abstract class BaseNewCommand extends Command {
  static paths: string[][];
  static usage: clipanion1.Usage;
  $$path: string | undefined;
  $$name?: string;
  minNodeApiVersion: number;
  packageManager: string;
  license: string;
  targets: string[];
  enableDefaultTargets: boolean;
  enableAllTargets: boolean;
  enableTypeDef: boolean;
  enableGithubActions: boolean;
  testFramework: string;
  dryRun: boolean;
  getOptions(): {
    path: string | undefined;
    name: string | undefined;
    minNodeApiVersion: number;
    packageManager: string;
    license: string;
    targets: string[];
    enableDefaultTargets: boolean;
    enableAllTargets: boolean;
    enableTypeDef: boolean;
    enableGithubActions: boolean;
    testFramework: string;
    dryRun: boolean;
  };
}
/**
 * Create a new project with pre-configured boilerplate
 */
interface NewOptions {
  /**
   * The path where the NAPI-RS project will be created.
   */
  path?: string;
  /**
   * The name of the project, default to the name of the directory if not provided
   */
  name?: string;
  /**
   * The minimum Node-API version to support
   *
   * @default 4
   */
  minNodeApiVersion?: number;
  /**
   * The package manager to use. Only support yarn 4.x for now.
   *
   * @default 'yarn'
   */
  packageManager?: string;
  /**
   * License for open-sourced project
   *
   * @default 'MIT'
   */
  license?: string;
  /**
   * All targets the crate will be compiled for.
   *
   * @default []
   */
  targets?: string[];
  /**
   * Whether enable default targets
   *
   * @default true
   */
  enableDefaultTargets?: boolean;
  /**
   * Whether enable all targets
   *
   * @default false
   */
  enableAllTargets?: boolean;
  /**
   * Whether enable the `type-def` feature for typescript definitions auto-generation
   *
   * @default true
   */
  enableTypeDef?: boolean;
  /**
   * Whether generate preconfigured GitHub Actions workflow
   *
   * @default true
   */
  enableGithubActions?: boolean;
  /**
   * The JavaScript test framework to use, only support `ava` for now
   *
   * @default 'ava'
   */
  testFramework?: string;
  /**
   * Whether to run the command in dry-run mode
   *
   * @default false
   */
  dryRun?: boolean;
}
//#endregion
//#region src/api/new.d.ts
declare function newProject(userOptions: NewOptions): Promise<void>;
//#endregion
//#region src/def/pre-publish.d.ts
declare abstract class BasePrePublishCommand extends Command {
  static paths: string[][];
  static usage: clipanion1.Usage;
  cwd: string;
  configPath?: string;
  packageJsonPath: string;
  npmDir: string;
  tagStyle: string;
  ghRelease: boolean;
  ghReleaseName?: string;
  ghReleaseId?: string;
  skipOptionalPublish: boolean;
  dryRun: boolean;
  getOptions(): {
    cwd: string;
    configPath: string | undefined;
    packageJsonPath: string;
    npmDir: string;
    tagStyle: string;
    ghRelease: boolean;
    ghReleaseName: string | undefined;
    ghReleaseId: string | undefined;
    skipOptionalPublish: boolean;
    dryRun: boolean;
  };
}
/**
 * Update package.json and copy addons into per platform packages
 */
interface PrePublishOptions {
  /**
   * The working directory of where napi command will be executed in, all other paths options are relative to this path
   *
   * @default process.cwd()
   */
  cwd?: string;
  /**
   * Path to `napi` config json file
   */
  configPath?: string;
  /**
   * Path to `package.json`
   *
   * @default 'package.json'
   */
  packageJsonPath?: string;
  /**
   * Path to the folder where the npm packages put
   *
   * @default 'npm'
   */
  npmDir?: string;
  /**
   * git tag style, `npm` or `lerna`
   *
   * @default 'lerna'
   */
  tagStyle?: 'npm' | 'lerna';
  /**
   * Whether create GitHub release
   *
   * @default true
   */
  ghRelease?: boolean;
  /**
   * GitHub release name
   */
  ghReleaseName?: string;
  /**
   * Existing GitHub release id
   */
  ghReleaseId?: string;
  /**
   * Whether skip optionalDependencies packages publish
   *
   * @default false
   */
  skipOptionalPublish?: boolean;
  /**
   * Dry run without touching file system
   *
   * @default false
   */
  dryRun?: boolean;
}
//#endregion
//#region src/api/pre-publish.d.ts
declare function prePublish(userOptions: PrePublishOptions): Promise<void>;
//#endregion
//#region src/def/rename.d.ts
declare abstract class BaseRenameCommand extends Command {
  static paths: string[][];
  static usage: clipanion1.Usage;
  cwd: string;
  configPath?: string;
  packageJsonPath: string;
  npmDir: string;
  $$name?: string;
  binaryName?: string;
  packageName?: string;
  manifestPath: string;
  repository?: string;
  description?: string;
  getOptions(): {
    cwd: string;
    configPath: string | undefined;
    packageJsonPath: string;
    npmDir: string;
    name: string | undefined;
    binaryName: string | undefined;
    packageName: string | undefined;
    manifestPath: string;
    repository: string | undefined;
    description: string | undefined;
  };
}
/**
 * Rename the NAPI-RS project
 */
interface RenameOptions {
  /**
   * The working directory of where napi command will be executed in, all other paths options are relative to this path
   *
   * @default process.cwd()
   */
  cwd?: string;
  /**
   * Path to `napi` config json file
   */
  configPath?: string;
  /**
   * Path to `package.json`
   *
   * @default 'package.json'
   */
  packageJsonPath?: string;
  /**
   * Path to the folder where the npm packages put
   *
   * @default 'npm'
   */
  npmDir?: string;
  /**
   * The new name of the project
   */
  name?: string;
  /**
   * The new binary name *.node files
   */
  binaryName?: string;
  /**
   * The new package name of the project
   */
  packageName?: string;
  /**
   * Path to `Cargo.toml`
   *
   * @default 'Cargo.toml'
   */
  manifestPath?: string;
  /**
   * The new repository of the project
   */
  repository?: string;
  /**
   * The new description of the project
   */
  description?: string;
}
//#endregion
//#region src/api/rename.d.ts
declare function renameProject(userOptions: RenameOptions): Promise<void>;
//#endregion
//#region src/def/universalize.d.ts
declare abstract class BaseUniversalizeCommand extends Command {
  static paths: string[][];
  static usage: clipanion1.Usage;
  cwd: string;
  configPath?: string;
  packageJsonPath: string;
  outputDir: string;
  getOptions(): {
    cwd: string;
    configPath: string | undefined;
    packageJsonPath: string;
    outputDir: string;
  };
}
/**
 * Combile built binaries into one universal binary
 */
interface UniversalizeOptions {
  /**
   * The working directory of where napi command will be executed in, all other paths options are relative to this path
   *
   * @default process.cwd()
   */
  cwd?: string;
  /**
   * Path to `napi` config json file
   */
  configPath?: string;
  /**
   * Path to `package.json`
   *
   * @default 'package.json'
   */
  packageJsonPath?: string;
  /**
   * Path to the folder where all built `.node` files put, same as `--output-dir` of build command
   *
   * @default './'
   */
  outputDir?: string;
}
//#endregion
//#region src/api/universalize.d.ts
declare function universalizeBinaries(userOptions: UniversalizeOptions): Promise<void>;
//#endregion
//#region src/def/version.d.ts
declare abstract class BaseVersionCommand extends Command {
  static paths: string[][];
  static usage: clipanion1.Usage;
  cwd: string;
  configPath?: string;
  packageJsonPath: string;
  npmDir: string;
  getOptions(): {
    cwd: string;
    configPath: string | undefined;
    packageJsonPath: string;
    npmDir: string;
  };
}
/**
 * Update version in created npm packages
 */
interface VersionOptions {
  /**
   * The working directory of where napi command will be executed in, all other paths options are relative to this path
   *
   * @default process.cwd()
   */
  cwd?: string;
  /**
   * Path to `napi` config json file
   */
  configPath?: string;
  /**
   * Path to `package.json`
   *
   * @default 'package.json'
   */
  packageJsonPath?: string;
  /**
   * Path to the folder where the npm packages put
   *
   * @default 'npm'
   */
  npmDir?: string;
}
//#endregion
//#region src/api/version.d.ts
declare function version(userOptions: VersionOptions): Promise<void>;
//#endregion
//#region src/commands/artifacts.d.ts
declare class ArtifactsCommand extends BaseArtifactsCommand {
  static usage: clipanion1.Usage;
  static paths: string[][];
  execute(): Promise<void>;
}
//#endregion
//#region src/commands/build.d.ts
declare class BuildCommand extends BaseBuildCommand {
  pipe: string | undefined;
  cargoOptions: string[];
  execute(): Promise<void>;
}
//#endregion
//#region src/commands/create-npm-dirs.d.ts
declare class CreateNpmDirsCommand extends BaseCreateNpmDirsCommand {
  execute(): Promise<void>;
}
//#endregion
//#region src/commands/new.d.ts
declare class NewCommand extends BaseNewCommand {
  interactive: boolean;
  execute(): Promise<0 | 1>;
  private fetchOptions;
  private fetchName;
  private fetchLicense;
  private fetchNapiVersion;
  private fetchTargets;
  private fetchTypeDef;
  private fetchGithubActions;
}
//#endregion
//#region src/commands/pre-publish.d.ts
declare class PrePublishCommand extends BasePrePublishCommand {
  execute(): Promise<void>;
}
//#endregion
//#region src/commands/rename.d.ts
declare class RenameCommand extends BaseRenameCommand {
  execute(): Promise<void>;
}
//#endregion
//#region src/commands/universalize.d.ts
declare class UniversalizeCommand extends BaseUniversalizeCommand {
  execute(): Promise<void>;
}
//#endregion
//#region src/commands/version.d.ts
declare class VersionCommand extends BaseVersionCommand {
  execute(): Promise<void>;
}
//#endregion
//#region src/utils/target.d.ts
type Platform = NodeJS.Platform | 'wasm' | 'wasi' | 'openharmony';
type NodeJSArch = 'arm' | 'arm64' | 'ia32' | 'loong64' | 'mips' | 'mipsel' | 'ppc' | 'ppc64' | 'riscv64' | 's390' | 's390x' | 'x32' | 'x64' | 'universal' | 'wasm32';
interface Target {
  triple: string;
  platformArchABI: string;
  platform: Platform;
  arch: NodeJSArch;
  abi: string | null;
}
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
declare function parseTriple(rawTriple: string): Target;
//#endregion
//#region src/utils/config.d.ts
interface UserNapiConfig {
  /**
   * Name of the binary to be generated, default to `index`
   */
  binaryName?: string;
  /**
   * Name of the npm package, default to the name of root package.json name
   *
   * Always given `@scope/pkg` and arch suffix will be appended like `@scope/pkg-linux-gnu-x64`
   */
  packageName?: string;
  /**
   * All targets the crate will be compiled for
   */
  targets?: string[];
  /**
   * The npm client project uses.
   */
  npmClient?: string;
  /**
   * Whether generate const enum for typescript bindings
   */
  constEnum?: boolean;
  /**
   * dts header prepend to the generated dts file
   */
  dtsHeader?: string;
  /**
   * dts header file path to be prepended to the generated dts file
   * if both dtsHeader and dtsHeaderFile are provided, dtsHeaderFile will be used
   */
  dtsHeaderFile?: string;
  /**
   * wasm compilation options
   */
  wasm?: {
    /**
     * https://developer.mozilla.org/en-US/docs/WebAssembly/JavaScript_interface/Memory
     * @default 4000 pages (256MiB)
     */
    initialMemory?: number;
    /**
     * @default 65536 pages (4GiB)
     */
    maximumMemory?: number;
    /**
     * Browser wasm binding configuration
     */
    browser: {
      /**
       * Whether to use fs module in browser
       */
      fs?: boolean;
      /**
       * Whether to initialize wasm asynchronously
       */
      asyncInit?: boolean;
      /**
       * Whether to inject `buffer` to emnapi context
       */
      buffer?: boolean;
    };
  };
  /**
   * @deprecated binaryName instead
   */
  name?: string;
  /**
   * @deprecated use packageName instead
   */
  package?: {
    name?: string;
  };
  /**
   * @deprecated use targets instead
   */
  triples?: {
    /**
     * Whether enable default targets
     */
    defaults: boolean;
    /**
     * Additional targets to be compiled for
     */
    additional?: string[];
  };
}
interface CommonPackageJsonFields {
  name: string;
  version: string;
  description?: string;
  keywords?: string[];
  author?: string;
  authors?: string[];
  license?: string;
  cpu?: string[];
  os?: string[];
  libc?: string[];
  files?: string[];
  repository?: any;
  homepage?: any;
  engines?: Record<string, string>;
  publishConfig?: any;
  bugs?: any;
  napi?: UserNapiConfig;
  type?: 'module' | 'commonjs';
  scripts?: Record<string, string>;
  main?: string;
  module?: string;
  types?: string;
  browser?: string;
  exports?: any;
  dependencies?: Record<string, string>;
  devDependencies?: Record<string, string>;
  ava?: {
    timeout?: string;
  };
}
type NapiConfig = Required<Pick<UserNapiConfig, 'binaryName' | 'packageName' | 'npmClient'>> & Pick<UserNapiConfig, 'wasm' | 'dtsHeader' | 'dtsHeaderFile' | 'constEnum'> & {
  targets: Target[];
  packageJson: CommonPackageJsonFields;
};
declare function readNapiConfig(path: string, configPath?: string): Promise<NapiConfig>;
//#endregion
//#region src/index.d.ts
declare const cli: Cli<clipanion1.BaseContext>;
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
declare class NapiCli {
  artifacts: typeof collectArtifacts;
  new: typeof newProject;
  build: typeof buildProject;
  createNpmDirs: typeof createNpmDirs;
  prePublish: typeof prePublish;
  rename: typeof renameProject;
  universalize: typeof universalizeBinaries;
  version: typeof version;
}
declare function createBuildCommand(args: string[]): BuildCommand;
declare function createArtifactsCommand(args: string[]): ArtifactsCommand;
declare function createCreateNpmDirsCommand(args: string[]): CreateNpmDirsCommand;
declare function createPrePublishCommand(args: string[]): PrePublishCommand;
declare function createRenameCommand(args: string[]): RenameCommand;
declare function createUniversalizeCommand(args: string[]): UniversalizeCommand;
declare function createVersionCommand(args: string[]): VersionCommand;
declare function createNewCommand(args: string[]): NewCommand;
//#endregion
export { type GenerateTypeDefOptions, NapiCli, type WriteJsBindingOptions, cli, createArtifactsCommand, createBuildCommand, createCreateNpmDirsCommand, createNewCommand, createPrePublishCommand, createRenameCommand, createUniversalizeCommand, createVersionCommand, generateTypeDef, parseTriple, readNapiConfig, writeJsBinding };
//# sourceMappingURL=data:application/json;charset=utf-8;base64,eyJ2ZXJzaW9uIjozLCJmaWxlIjoiaW5kZXguZC5jdHMiLCJuYW1lcyI6W10sInNvdXJjZXMiOlsiLi4vc3JjL2RlZi9hcnRpZmFjdHMuZC50cyIsIi4uL3NyYy9hcGkvYXJ0aWZhY3RzLmQudHMiLCIuLi9zcmMvZGVmL2J1aWxkLmQudHMiLCIuLi9zcmMvYXBpL2J1aWxkLmQudHMiLCIuLi9zcmMvZGVmL2NyZWF0ZS1ucG0tZGlycy5kLnRzIiwiLi4vc3JjL2FwaS9jcmVhdGUtbnBtLWRpcnMuZC50cyIsIi4uL3NyYy9kZWYvbmV3LmQudHMiLCIuLi9zcmMvYXBpL25ldy5kLnRzIiwiLi4vc3JjL2RlZi9wcmUtcHVibGlzaC5kLnRzIiwiLi4vc3JjL2FwaS9wcmUtcHVibGlzaC5kLnRzIiwiLi4vc3JjL2RlZi9yZW5hbWUuZC50cyIsIi4uL3NyYy9hcGkvcmVuYW1lLmQudHMiLCIuLi9zcmMvZGVmL3VuaXZlcnNhbGl6ZS5kLnRzIiwiLi4vc3JjL2FwaS91bml2ZXJzYWxpemUuZC50cyIsIi4uL3NyYy9kZWYvdmVyc2lvbi5kLnRzIiwiLi4vc3JjL2FwaS92ZXJzaW9uLmQudHMiLCIuLi9zcmMvY29tbWFuZHMvYXJ0aWZhY3RzLmQudHMiLCIuLi9zcmMvY29tbWFuZHMvYnVpbGQuZC50cyIsIi4uL3NyYy9jb21tYW5kcy9jcmVhdGUtbnBtLWRpcnMuZC50cyIsIi4uL3NyYy9jb21tYW5kcy9uZXcuZC50cyIsIi4uL3NyYy9jb21tYW5kcy9wcmUtcHVibGlzaC5kLnRzIiwiLi4vc3JjL2NvbW1hbmRzL3JlbmFtZS5kLnRzIiwiLi4vc3JjL2NvbW1hbmRzL3VuaXZlcnNhbGl6ZS5kLnRzIiwiLi4vc3JjL2NvbW1hbmRzL3ZlcnNpb24uZC50cyIsIi4uL3NyYy91dGlscy90YXJnZXQuZC50cyIsIi4uL3NyYy91dGlscy9jb25maWcuZC50cyIsIi4uL3NyYy9pbmRleC5kLnRzIl0sInNvdXJjZXNDb250ZW50IjpbImltcG9ydCB7IENvbW1hbmQgfSBmcm9tICdjbGlwYW5pb24nO1xuZXhwb3J0IGRlY2xhcmUgYWJzdHJhY3QgY2xhc3MgQmFzZUFydGlmYWN0c0NvbW1hbmQgZXh0ZW5kcyBDb21tYW5kIHtcbiAgICBzdGF0aWMgcGF0aHM6IHN0cmluZ1tdW107XG4gICAgc3RhdGljIHVzYWdlOiBpbXBvcnQoXCJjbGlwYW5pb25cIikuVXNhZ2U7XG4gICAgY3dkOiBzdHJpbmc7XG4gICAgY29uZmlnUGF0aD86IHN0cmluZztcbiAgICBwYWNrYWdlSnNvblBhdGg6IHN0cmluZztcbiAgICBvdXRwdXREaXI6IHN0cmluZztcbiAgICBucG1EaXI6IHN0cmluZztcbiAgICBidWlsZE91dHB1dERpcj86IHN0cmluZztcbiAgICBnZXRPcHRpb25zKCk6IHtcbiAgICAgICAgY3dkOiBzdHJpbmc7XG4gICAgICAgIGNvbmZpZ1BhdGg6IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICAgICAgcGFja2FnZUpzb25QYXRoOiBzdHJpbmc7XG4gICAgICAgIG91dHB1dERpcjogc3RyaW5nO1xuICAgICAgICBucG1EaXI6IHN0cmluZztcbiAgICAgICAgYnVpbGRPdXRwdXREaXI6IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICB9O1xufVxuLyoqXG4gKiBDb3B5IGFydGlmYWN0cyBmcm9tIEdpdGh1YiBBY3Rpb25zIGludG8gbnBtIHBhY2thZ2VzIGFuZCByZWFkeSB0byBwdWJsaXNoXG4gKi9cbmV4cG9ydCBpbnRlcmZhY2UgQXJ0aWZhY3RzT3B0aW9ucyB7XG4gICAgLyoqXG4gICAgICogVGhlIHdvcmtpbmcgZGlyZWN0b3J5IG9mIHdoZXJlIG5hcGkgY29tbWFuZCB3aWxsIGJlIGV4ZWN1dGVkIGluLCBhbGwgb3RoZXIgcGF0aHMgb3B0aW9ucyBhcmUgcmVsYXRpdmUgdG8gdGhpcyBwYXRoXG4gICAgICpcbiAgICAgKiBAZGVmYXVsdCBwcm9jZXNzLmN3ZCgpXG4gICAgICovXG4gICAgY3dkPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gYG5hcGlgIGNvbmZpZyBqc29uIGZpbGVcbiAgICAgKi9cbiAgICBjb25maWdQYXRoPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gYHBhY2thZ2UuanNvbmBcbiAgICAgKlxuICAgICAqIEBkZWZhdWx0ICdwYWNrYWdlLmpzb24nXG4gICAgICovXG4gICAgcGFja2FnZUpzb25QYXRoPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gdGhlIGZvbGRlciB3aGVyZSBhbGwgYnVpbHQgYC5ub2RlYCBmaWxlcyBwdXQsIHNhbWUgYXMgYC0tb3V0cHV0LWRpcmAgb2YgYnVpbGQgY29tbWFuZFxuICAgICAqXG4gICAgICogQGRlZmF1bHQgJy4vYXJ0aWZhY3RzJ1xuICAgICAqL1xuICAgIG91dHB1dERpcj86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIHRoZSBmb2xkZXIgd2hlcmUgdGhlIG5wbSBwYWNrYWdlcyBwdXRcbiAgICAgKlxuICAgICAqIEBkZWZhdWx0ICducG0nXG4gICAgICovXG4gICAgbnBtRGlyPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gdGhlIGJ1aWxkIG91dHB1dCBkaXIsIG9ubHkgbmVlZGVkIHdoZW4gdGFyZ2V0cyBjb250YWlucyBgd2FzbTMyLXdhc2ktKmBcbiAgICAgKi9cbiAgICBidWlsZE91dHB1dERpcj86IHN0cmluZztcbn1cbmV4cG9ydCBkZWNsYXJlIGZ1bmN0aW9uIGFwcGx5RGVmYXVsdEFydGlmYWN0c09wdGlvbnMob3B0aW9uczogQXJ0aWZhY3RzT3B0aW9ucyk6IHtcbiAgICBjd2Q6IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIGBuYXBpYCBjb25maWcganNvbiBmaWxlXG4gICAgICovXG4gICAgY29uZmlnUGF0aD86IHN0cmluZztcbiAgICBwYWNrYWdlSnNvblBhdGg6IHN0cmluZztcbiAgICBvdXRwdXREaXI6IHN0cmluZztcbiAgICBucG1EaXI6IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIHRoZSBidWlsZCBvdXRwdXQgZGlyLCBvbmx5IG5lZWRlZCB3aGVuIHRhcmdldHMgY29udGFpbnMgYHdhc20zMi13YXNpLSpgXG4gICAgICovXG4gICAgYnVpbGRPdXRwdXREaXI/OiBzdHJpbmc7XG59O1xuIiwiaW1wb3J0IHsgdHlwZSBBcnRpZmFjdHNPcHRpb25zIH0gZnJvbSAnLi4vZGVmL2FydGlmYWN0cy5qcyc7XG5leHBvcnQgZGVjbGFyZSBmdW5jdGlvbiBjb2xsZWN0QXJ0aWZhY3RzKHVzZXJPcHRpb25zOiBBcnRpZmFjdHNPcHRpb25zKTogUHJvbWlzZTx2b2lkPjtcbiIsImltcG9ydCB7IENvbW1hbmQgfSBmcm9tICdjbGlwYW5pb24nO1xuZXhwb3J0IGRlY2xhcmUgYWJzdHJhY3QgY2xhc3MgQmFzZUJ1aWxkQ29tbWFuZCBleHRlbmRzIENvbW1hbmQge1xuICAgIHN0YXRpYyBwYXRoczogc3RyaW5nW11bXTtcbiAgICBzdGF0aWMgdXNhZ2U6IGltcG9ydChcImNsaXBhbmlvblwiKS5Vc2FnZTtcbiAgICB0YXJnZXQ/OiBzdHJpbmc7XG4gICAgY3dkPzogc3RyaW5nO1xuICAgIG1hbmlmZXN0UGF0aD86IHN0cmluZztcbiAgICBjb25maWdQYXRoPzogc3RyaW5nO1xuICAgIHBhY2thZ2VKc29uUGF0aD86IHN0cmluZztcbiAgICB0YXJnZXREaXI/OiBzdHJpbmc7XG4gICAgb3V0cHV0RGlyPzogc3RyaW5nO1xuICAgIHBsYXRmb3JtPzogYm9vbGVhbjtcbiAgICBqc1BhY2thZ2VOYW1lPzogc3RyaW5nO1xuICAgIGNvbnN0RW51bT86IGJvb2xlYW47XG4gICAganNCaW5kaW5nPzogc3RyaW5nO1xuICAgIG5vSnNCaW5kaW5nPzogYm9vbGVhbjtcbiAgICBkdHM/OiBzdHJpbmc7XG4gICAgZHRzSGVhZGVyPzogc3RyaW5nO1xuICAgIG5vRHRzSGVhZGVyPzogYm9vbGVhbjtcbiAgICBkdHNDYWNoZTogYm9vbGVhbjtcbiAgICBlc20/OiBib29sZWFuO1xuICAgIHN0cmlwPzogYm9vbGVhbjtcbiAgICByZWxlYXNlPzogYm9vbGVhbjtcbiAgICB2ZXJib3NlPzogYm9vbGVhbjtcbiAgICBiaW4/OiBzdHJpbmc7XG4gICAgcGFja2FnZT86IHN0cmluZztcbiAgICBwcm9maWxlPzogc3RyaW5nO1xuICAgIGNyb3NzQ29tcGlsZT86IGJvb2xlYW47XG4gICAgdXNlQ3Jvc3M/OiBib29sZWFuO1xuICAgIHVzZU5hcGlDcm9zcz86IGJvb2xlYW47XG4gICAgd2F0Y2g/OiBib29sZWFuO1xuICAgIGZlYXR1cmVzPzogc3RyaW5nW107XG4gICAgYWxsRmVhdHVyZXM/OiBib29sZWFuO1xuICAgIG5vRGVmYXVsdEZlYXR1cmVzPzogYm9vbGVhbjtcbiAgICBnZXRPcHRpb25zKCk6IHtcbiAgICAgICAgdGFyZ2V0OiBzdHJpbmcgfCB1bmRlZmluZWQ7XG4gICAgICAgIGN3ZDogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgICAgICBtYW5pZmVzdFBhdGg6IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICAgICAgY29uZmlnUGF0aDogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgICAgICBwYWNrYWdlSnNvblBhdGg6IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICAgICAgdGFyZ2V0RGlyOiBzdHJpbmcgfCB1bmRlZmluZWQ7XG4gICAgICAgIG91dHB1dERpcjogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgICAgICBwbGF0Zm9ybTogYm9vbGVhbiB8IHVuZGVmaW5lZDtcbiAgICAgICAganNQYWNrYWdlTmFtZTogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgICAgICBjb25zdEVudW06IGJvb2xlYW4gfCB1bmRlZmluZWQ7XG4gICAgICAgIGpzQmluZGluZzogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgICAgICBub0pzQmluZGluZzogYm9vbGVhbiB8IHVuZGVmaW5lZDtcbiAgICAgICAgZHRzOiBzdHJpbmcgfCB1bmRlZmluZWQ7XG4gICAgICAgIGR0c0hlYWRlcjogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgICAgICBub0R0c0hlYWRlcjogYm9vbGVhbiB8IHVuZGVmaW5lZDtcbiAgICAgICAgZHRzQ2FjaGU6IGJvb2xlYW47XG4gICAgICAgIGVzbTogYm9vbGVhbiB8IHVuZGVmaW5lZDtcbiAgICAgICAgc3RyaXA6IGJvb2xlYW4gfCB1bmRlZmluZWQ7XG4gICAgICAgIHJlbGVhc2U6IGJvb2xlYW4gfCB1bmRlZmluZWQ7XG4gICAgICAgIHZlcmJvc2U6IGJvb2xlYW4gfCB1bmRlZmluZWQ7XG4gICAgICAgIGJpbjogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgICAgICBwYWNrYWdlOiBzdHJpbmcgfCB1bmRlZmluZWQ7XG4gICAgICAgIHByb2ZpbGU6IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICAgICAgY3Jvc3NDb21waWxlOiBib29sZWFuIHwgdW5kZWZpbmVkO1xuICAgICAgICB1c2VDcm9zczogYm9vbGVhbiB8IHVuZGVmaW5lZDtcbiAgICAgICAgdXNlTmFwaUNyb3NzOiBib29sZWFuIHwgdW5kZWZpbmVkO1xuICAgICAgICB3YXRjaDogYm9vbGVhbiB8IHVuZGVmaW5lZDtcbiAgICAgICAgZmVhdHVyZXM6IHN0cmluZ1tdIHwgdW5kZWZpbmVkO1xuICAgICAgICBhbGxGZWF0dXJlczogYm9vbGVhbiB8IHVuZGVmaW5lZDtcbiAgICAgICAgbm9EZWZhdWx0RmVhdHVyZXM6IGJvb2xlYW4gfCB1bmRlZmluZWQ7XG4gICAgfTtcbn1cbi8qKlxuICogQnVpbGQgdGhlIE5BUEktUlMgcHJvamVjdFxuICovXG5leHBvcnQgaW50ZXJmYWNlIEJ1aWxkT3B0aW9ucyB7XG4gICAgLyoqXG4gICAgICogQnVpbGQgZm9yIHRoZSB0YXJnZXQgdHJpcGxlLCBieXBhc3NlZCB0byBgY2FyZ28gYnVpbGQgLS10YXJnZXRgXG4gICAgICovXG4gICAgdGFyZ2V0Pzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFRoZSB3b3JraW5nIGRpcmVjdG9yeSBvZiB3aGVyZSBuYXBpIGNvbW1hbmQgd2lsbCBiZSBleGVjdXRlZCBpbiwgYWxsIG90aGVyIHBhdGhzIG9wdGlvbnMgYXJlIHJlbGF0aXZlIHRvIHRoaXMgcGF0aFxuICAgICAqL1xuICAgIGN3ZD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIGBDYXJnby50b21sYFxuICAgICAqL1xuICAgIG1hbmlmZXN0UGF0aD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIGBuYXBpYCBjb25maWcganNvbiBmaWxlXG4gICAgICovXG4gICAgY29uZmlnUGF0aD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIGBwYWNrYWdlLmpzb25gXG4gICAgICovXG4gICAgcGFja2FnZUpzb25QYXRoPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIERpcmVjdG9yeSBmb3IgYWxsIGNyYXRlIGdlbmVyYXRlZCBhcnRpZmFjdHMsIHNlZSBgY2FyZ28gYnVpbGQgLS10YXJnZXQtZGlyYFxuICAgICAqL1xuICAgIHRhcmdldERpcj86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIHdoZXJlIGFsbCB0aGUgYnVpbHQgZmlsZXMgd291bGQgYmUgcHV0LiBEZWZhdWx0IHRvIHRoZSBjcmF0ZSBmb2xkZXJcbiAgICAgKi9cbiAgICBvdXRwdXREaXI/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogQWRkIHBsYXRmb3JtIHRyaXBsZSB0byB0aGUgZ2VuZXJhdGVkIG5vZGVqcyBiaW5kaW5nIGZpbGUsIGVnOiBgW25hbWVdLmxpbnV4LXg2NC1nbnUubm9kZWBcbiAgICAgKi9cbiAgICBwbGF0Zm9ybT86IGJvb2xlYW47XG4gICAgLyoqXG4gICAgICogUGFja2FnZSBuYW1lIGluIGdlbmVyYXRlZCBqcyBiaW5kaW5nIGZpbGUuIE9ubHkgd29ya3Mgd2l0aCBgLS1wbGF0Zm9ybWAgZmxhZ1xuICAgICAqL1xuICAgIGpzUGFja2FnZU5hbWU/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogV2hldGhlciBnZW5lcmF0ZSBjb25zdCBlbnVtIGZvciB0eXBlc2NyaXB0IGJpbmRpbmdzXG4gICAgICovXG4gICAgY29uc3RFbnVtPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBQYXRoIGFuZCBmaWxlbmFtZSBvZiBnZW5lcmF0ZWQgSlMgYmluZGluZyBmaWxlLiBPbmx5IHdvcmtzIHdpdGggYC0tcGxhdGZvcm1gIGZsYWcuIFJlbGF0aXZlIHRvIGAtLW91dHB1dC1kaXJgLlxuICAgICAqL1xuICAgIGpzQmluZGluZz86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBXaGV0aGVyIHRvIGRpc2FibGUgdGhlIGdlbmVyYXRpb24gSlMgYmluZGluZyBmaWxlLiBPbmx5IHdvcmtzIHdpdGggYC0tcGxhdGZvcm1gIGZsYWcuXG4gICAgICovXG4gICAgbm9Kc0JpbmRpbmc/OiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIFBhdGggYW5kIGZpbGVuYW1lIG9mIGdlbmVyYXRlZCB0eXBlIGRlZiBmaWxlLiBSZWxhdGl2ZSB0byBgLS1vdXRwdXQtZGlyYFxuICAgICAqL1xuICAgIGR0cz86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBDdXN0b20gZmlsZSBoZWFkZXIgZm9yIGdlbmVyYXRlZCB0eXBlIGRlZiBmaWxlLiBPbmx5IHdvcmtzIHdoZW4gYHR5cGVkZWZgIGZlYXR1cmUgZW5hYmxlZC5cbiAgICAgKi9cbiAgICBkdHNIZWFkZXI/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogV2hldGhlciB0byBkaXNhYmxlIHRoZSBkZWZhdWx0IGZpbGUgaGVhZGVyIGZvciBnZW5lcmF0ZWQgdHlwZSBkZWYgZmlsZS4gT25seSB3b3JrcyB3aGVuIGB0eXBlZGVmYCBmZWF0dXJlIGVuYWJsZWQuXG4gICAgICovXG4gICAgbm9EdHNIZWFkZXI/OiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIFdoZXRoZXIgdG8gZW5hYmxlIHRoZSBkdHMgY2FjaGUsIGRlZmF1bHQgdG8gdHJ1ZVxuICAgICAqXG4gICAgICogQGRlZmF1bHQgdHJ1ZVxuICAgICAqL1xuICAgIGR0c0NhY2hlPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBXaGV0aGVyIHRvIGVtaXQgYW4gRVNNIEpTIGJpbmRpbmcgZmlsZSBpbnN0ZWFkIG9mIENKUyBmb3JtYXQuIE9ubHkgd29ya3Mgd2l0aCBgLS1wbGF0Zm9ybWAgZmxhZy5cbiAgICAgKi9cbiAgICBlc20/OiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIFdoZXRoZXIgc3RyaXAgdGhlIGxpYnJhcnkgdG8gYWNoaWV2ZSB0aGUgbWluaW11bSBmaWxlIHNpemVcbiAgICAgKi9cbiAgICBzdHJpcD86IGJvb2xlYW47XG4gICAgLyoqXG4gICAgICogQnVpbGQgaW4gcmVsZWFzZSBtb2RlXG4gICAgICovXG4gICAgcmVsZWFzZT86IGJvb2xlYW47XG4gICAgLyoqXG4gICAgICogVmVyYm9zZWx5IGxvZyBidWlsZCBjb21tYW5kIHRyYWNlXG4gICAgICovXG4gICAgdmVyYm9zZT86IGJvb2xlYW47XG4gICAgLyoqXG4gICAgICogQnVpbGQgb25seSB0aGUgc3BlY2lmaWVkIGJpbmFyeVxuICAgICAqL1xuICAgIGJpbj86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBCdWlsZCB0aGUgc3BlY2lmaWVkIGxpYnJhcnkgb3IgdGhlIG9uZSBhdCBjd2RcbiAgICAgKi9cbiAgICBwYWNrYWdlPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIEJ1aWxkIGFydGlmYWN0cyB3aXRoIHRoZSBzcGVjaWZpZWQgcHJvZmlsZVxuICAgICAqL1xuICAgIHByb2ZpbGU/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogW2V4cGVyaW1lbnRhbF0gY3Jvc3MtY29tcGlsZSBmb3IgdGhlIHNwZWNpZmllZCB0YXJnZXQgd2l0aCBgY2FyZ28teHdpbmAgb24gd2luZG93cyBhbmQgYGNhcmdvLXppZ2J1aWxkYCBvbiBvdGhlciBwbGF0Zm9ybVxuICAgICAqL1xuICAgIGNyb3NzQ29tcGlsZT86IGJvb2xlYW47XG4gICAgLyoqXG4gICAgICogW2V4cGVyaW1lbnRhbF0gdXNlIFtjcm9zc10oaHR0cHM6Ly9naXRodWIuY29tL2Nyb3NzLXJzL2Nyb3NzKSBpbnN0ZWFkIG9mIGBjYXJnb2BcbiAgICAgKi9cbiAgICB1c2VDcm9zcz86IGJvb2xlYW47XG4gICAgLyoqXG4gICAgICogW2V4cGVyaW1lbnRhbF0gdXNlIEBuYXBpLXJzL2Nyb3NzLXRvb2xjaGFpbiB0byBjcm9zcy1jb21waWxlIExpbnV4IGFybS9hcm02NC94NjQgZ251IHRhcmdldHMuXG4gICAgICovXG4gICAgdXNlTmFwaUNyb3NzPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiB3YXRjaCB0aGUgY3JhdGUgY2hhbmdlcyBhbmQgYnVpbGQgY29udGludW91c2x5IHdpdGggYGNhcmdvLXdhdGNoYCBjcmF0ZXNcbiAgICAgKi9cbiAgICB3YXRjaD86IGJvb2xlYW47XG4gICAgLyoqXG4gICAgICogU3BhY2Utc2VwYXJhdGVkIGxpc3Qgb2YgZmVhdHVyZXMgdG8gYWN0aXZhdGVcbiAgICAgKi9cbiAgICBmZWF0dXJlcz86IHN0cmluZ1tdO1xuICAgIC8qKlxuICAgICAqIEFjdGl2YXRlIGFsbCBhdmFpbGFibGUgZmVhdHVyZXNcbiAgICAgKi9cbiAgICBhbGxGZWF0dXJlcz86IGJvb2xlYW47XG4gICAgLyoqXG4gICAgICogRG8gbm90IGFjdGl2YXRlIHRoZSBgZGVmYXVsdGAgZmVhdHVyZVxuICAgICAqL1xuICAgIG5vRGVmYXVsdEZlYXR1cmVzPzogYm9vbGVhbjtcbn1cbmV4cG9ydCBkZWNsYXJlIGZ1bmN0aW9uIGFwcGx5RGVmYXVsdEJ1aWxkT3B0aW9ucyhvcHRpb25zOiBCdWlsZE9wdGlvbnMpOiB7XG4gICAgLyoqXG4gICAgICogQnVpbGQgZm9yIHRoZSB0YXJnZXQgdHJpcGxlLCBieXBhc3NlZCB0byBgY2FyZ28gYnVpbGQgLS10YXJnZXRgXG4gICAgICovXG4gICAgdGFyZ2V0Pzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFRoZSB3b3JraW5nIGRpcmVjdG9yeSBvZiB3aGVyZSBuYXBpIGNvbW1hbmQgd2lsbCBiZSBleGVjdXRlZCBpbiwgYWxsIG90aGVyIHBhdGhzIG9wdGlvbnMgYXJlIHJlbGF0aXZlIHRvIHRoaXMgcGF0aFxuICAgICAqL1xuICAgIGN3ZD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIGBDYXJnby50b21sYFxuICAgICAqL1xuICAgIG1hbmlmZXN0UGF0aD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIGBuYXBpYCBjb25maWcganNvbiBmaWxlXG4gICAgICovXG4gICAgY29uZmlnUGF0aD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIGBwYWNrYWdlLmpzb25gXG4gICAgICovXG4gICAgcGFja2FnZUpzb25QYXRoPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIERpcmVjdG9yeSBmb3IgYWxsIGNyYXRlIGdlbmVyYXRlZCBhcnRpZmFjdHMsIHNlZSBgY2FyZ28gYnVpbGQgLS10YXJnZXQtZGlyYFxuICAgICAqL1xuICAgIHRhcmdldERpcj86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIHdoZXJlIGFsbCB0aGUgYnVpbHQgZmlsZXMgd291bGQgYmUgcHV0LiBEZWZhdWx0IHRvIHRoZSBjcmF0ZSBmb2xkZXJcbiAgICAgKi9cbiAgICBvdXRwdXREaXI/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogQWRkIHBsYXRmb3JtIHRyaXBsZSB0byB0aGUgZ2VuZXJhdGVkIG5vZGVqcyBiaW5kaW5nIGZpbGUsIGVnOiBgW25hbWVdLmxpbnV4LXg2NC1nbnUubm9kZWBcbiAgICAgKi9cbiAgICBwbGF0Zm9ybT86IGJvb2xlYW47XG4gICAgLyoqXG4gICAgICogUGFja2FnZSBuYW1lIGluIGdlbmVyYXRlZCBqcyBiaW5kaW5nIGZpbGUuIE9ubHkgd29ya3Mgd2l0aCBgLS1wbGF0Zm9ybWAgZmxhZ1xuICAgICAqL1xuICAgIGpzUGFja2FnZU5hbWU/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogV2hldGhlciBnZW5lcmF0ZSBjb25zdCBlbnVtIGZvciB0eXBlc2NyaXB0IGJpbmRpbmdzXG4gICAgICovXG4gICAgY29uc3RFbnVtPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBQYXRoIGFuZCBmaWxlbmFtZSBvZiBnZW5lcmF0ZWQgSlMgYmluZGluZyBmaWxlLiBPbmx5IHdvcmtzIHdpdGggYC0tcGxhdGZvcm1gIGZsYWcuIFJlbGF0aXZlIHRvIGAtLW91dHB1dC1kaXJgLlxuICAgICAqL1xuICAgIGpzQmluZGluZz86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBXaGV0aGVyIHRvIGRpc2FibGUgdGhlIGdlbmVyYXRpb24gSlMgYmluZGluZyBmaWxlLiBPbmx5IHdvcmtzIHdpdGggYC0tcGxhdGZvcm1gIGZsYWcuXG4gICAgICovXG4gICAgbm9Kc0JpbmRpbmc/OiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIFBhdGggYW5kIGZpbGVuYW1lIG9mIGdlbmVyYXRlZCB0eXBlIGRlZiBmaWxlLiBSZWxhdGl2ZSB0byBgLS1vdXRwdXQtZGlyYFxuICAgICAqL1xuICAgIGR0cz86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBDdXN0b20gZmlsZSBoZWFkZXIgZm9yIGdlbmVyYXRlZCB0eXBlIGRlZiBmaWxlLiBPbmx5IHdvcmtzIHdoZW4gYHR5cGVkZWZgIGZlYXR1cmUgZW5hYmxlZC5cbiAgICAgKi9cbiAgICBkdHNIZWFkZXI/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogV2hldGhlciB0byBkaXNhYmxlIHRoZSBkZWZhdWx0IGZpbGUgaGVhZGVyIGZvciBnZW5lcmF0ZWQgdHlwZSBkZWYgZmlsZS4gT25seSB3b3JrcyB3aGVuIGB0eXBlZGVmYCBmZWF0dXJlIGVuYWJsZWQuXG4gICAgICovXG4gICAgbm9EdHNIZWFkZXI/OiBib29sZWFuO1xuICAgIGR0c0NhY2hlOiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIFdoZXRoZXIgdG8gZW1pdCBhbiBFU00gSlMgYmluZGluZyBmaWxlIGluc3RlYWQgb2YgQ0pTIGZvcm1hdC4gT25seSB3b3JrcyB3aXRoIGAtLXBsYXRmb3JtYCBmbGFnLlxuICAgICAqL1xuICAgIGVzbT86IGJvb2xlYW47XG4gICAgLyoqXG4gICAgICogV2hldGhlciBzdHJpcCB0aGUgbGlicmFyeSB0byBhY2hpZXZlIHRoZSBtaW5pbXVtIGZpbGUgc2l6ZVxuICAgICAqL1xuICAgIHN0cmlwPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBCdWlsZCBpbiByZWxlYXNlIG1vZGVcbiAgICAgKi9cbiAgICByZWxlYXNlPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBWZXJib3NlbHkgbG9nIGJ1aWxkIGNvbW1hbmQgdHJhY2VcbiAgICAgKi9cbiAgICB2ZXJib3NlPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBCdWlsZCBvbmx5IHRoZSBzcGVjaWZpZWQgYmluYXJ5XG4gICAgICovXG4gICAgYmluPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIEJ1aWxkIHRoZSBzcGVjaWZpZWQgbGlicmFyeSBvciB0aGUgb25lIGF0IGN3ZFxuICAgICAqL1xuICAgIHBhY2thZ2U/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogQnVpbGQgYXJ0aWZhY3RzIHdpdGggdGhlIHNwZWNpZmllZCBwcm9maWxlXG4gICAgICovXG4gICAgcHJvZmlsZT86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBbZXhwZXJpbWVudGFsXSBjcm9zcy1jb21waWxlIGZvciB0aGUgc3BlY2lmaWVkIHRhcmdldCB3aXRoIGBjYXJnby14d2luYCBvbiB3aW5kb3dzIGFuZCBgY2FyZ28temlnYnVpbGRgIG9uIG90aGVyIHBsYXRmb3JtXG4gICAgICovXG4gICAgY3Jvc3NDb21waWxlPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBbZXhwZXJpbWVudGFsXSB1c2UgW2Nyb3NzXShodHRwczovL2dpdGh1Yi5jb20vY3Jvc3MtcnMvY3Jvc3MpIGluc3RlYWQgb2YgYGNhcmdvYFxuICAgICAqL1xuICAgIHVzZUNyb3NzPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBbZXhwZXJpbWVudGFsXSB1c2UgQG5hcGktcnMvY3Jvc3MtdG9vbGNoYWluIHRvIGNyb3NzLWNvbXBpbGUgTGludXggYXJtL2FybTY0L3g2NCBnbnUgdGFyZ2V0cy5cbiAgICAgKi9cbiAgICB1c2VOYXBpQ3Jvc3M/OiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIHdhdGNoIHRoZSBjcmF0ZSBjaGFuZ2VzIGFuZCBidWlsZCBjb250aW51b3VzbHkgd2l0aCBgY2FyZ28td2F0Y2hgIGNyYXRlc1xuICAgICAqL1xuICAgIHdhdGNoPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBTcGFjZS1zZXBhcmF0ZWQgbGlzdCBvZiBmZWF0dXJlcyB0byBhY3RpdmF0ZVxuICAgICAqL1xuICAgIGZlYXR1cmVzPzogc3RyaW5nW107XG4gICAgLyoqXG4gICAgICogQWN0aXZhdGUgYWxsIGF2YWlsYWJsZSBmZWF0dXJlc1xuICAgICAqL1xuICAgIGFsbEZlYXR1cmVzPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBEbyBub3QgYWN0aXZhdGUgdGhlIGBkZWZhdWx0YCBmZWF0dXJlXG4gICAgICovXG4gICAgbm9EZWZhdWx0RmVhdHVyZXM/OiBib29sZWFuO1xufTtcbiIsImltcG9ydCB0eXBlIHsgQnVpbGRPcHRpb25zIGFzIFJhd0J1aWxkT3B0aW9ucyB9IGZyb20gJy4uL2RlZi9idWlsZC5qcyc7XG50eXBlIE91dHB1dEtpbmQgPSAnanMnIHwgJ2R0cycgfCAnbm9kZScgfCAnZXhlJyB8ICd3YXNtJztcbnR5cGUgT3V0cHV0ID0ge1xuICAgIGtpbmQ6IE91dHB1dEtpbmQ7XG4gICAgcGF0aDogc3RyaW5nO1xufTtcbnR5cGUgQnVpbGRPcHRpb25zID0gUmF3QnVpbGRPcHRpb25zICYge1xuICAgIGNhcmdvT3B0aW9ucz86IHN0cmluZ1tdO1xufTtcbmV4cG9ydCBkZWNsYXJlIGZ1bmN0aW9uIGJ1aWxkUHJvamVjdChyYXdPcHRpb25zOiBCdWlsZE9wdGlvbnMpOiBQcm9taXNlPHtcbiAgICB0YXNrOiBQcm9taXNlPE91dHB1dFtdPjtcbiAgICBhYm9ydDogKCkgPT4gdm9pZDtcbn0+O1xuZXhwb3J0IGludGVyZmFjZSBXcml0ZUpzQmluZGluZ09wdGlvbnMge1xuICAgIHBsYXRmb3JtPzogYm9vbGVhbjtcbiAgICBub0pzQmluZGluZz86IGJvb2xlYW47XG4gICAgaWRlbnRzOiBzdHJpbmdbXTtcbiAgICBqc0JpbmRpbmc/OiBzdHJpbmc7XG4gICAgZXNtPzogYm9vbGVhbjtcbiAgICBiaW5hcnlOYW1lOiBzdHJpbmc7XG4gICAgcGFja2FnZU5hbWU6IHN0cmluZztcbiAgICB2ZXJzaW9uOiBzdHJpbmc7XG4gICAgb3V0cHV0RGlyOiBzdHJpbmc7XG59XG5leHBvcnQgZGVjbGFyZSBmdW5jdGlvbiB3cml0ZUpzQmluZGluZyhvcHRpb25zOiBXcml0ZUpzQmluZGluZ09wdGlvbnMpOiBQcm9taXNlPE91dHB1dCB8IHVuZGVmaW5lZD47XG5leHBvcnQgaW50ZXJmYWNlIEdlbmVyYXRlVHlwZURlZk9wdGlvbnMge1xuICAgIHR5cGVEZWZEaXI6IHN0cmluZztcbiAgICBub0R0c0hlYWRlcj86IGJvb2xlYW47XG4gICAgZHRzSGVhZGVyPzogc3RyaW5nO1xuICAgIGR0c0hlYWRlckZpbGU/OiBzdHJpbmc7XG4gICAgY29uZmlnRHRzSGVhZGVyPzogc3RyaW5nO1xuICAgIGNvbmZpZ0R0c0hlYWRlckZpbGU/OiBzdHJpbmc7XG4gICAgY29uc3RFbnVtPzogYm9vbGVhbjtcbiAgICBjd2Q6IHN0cmluZztcbn1cbmV4cG9ydCBkZWNsYXJlIGZ1bmN0aW9uIGdlbmVyYXRlVHlwZURlZihvcHRpb25zOiBHZW5lcmF0ZVR5cGVEZWZPcHRpb25zKTogUHJvbWlzZTx7XG4gICAgZXhwb3J0czogc3RyaW5nW107XG4gICAgZHRzOiBzdHJpbmc7XG59PjtcbmV4cG9ydCB7fTtcbiIsImltcG9ydCB7IENvbW1hbmQgfSBmcm9tICdjbGlwYW5pb24nO1xuZXhwb3J0IGRlY2xhcmUgYWJzdHJhY3QgY2xhc3MgQmFzZUNyZWF0ZU5wbURpcnNDb21tYW5kIGV4dGVuZHMgQ29tbWFuZCB7XG4gICAgc3RhdGljIHBhdGhzOiBzdHJpbmdbXVtdO1xuICAgIHN0YXRpYyB1c2FnZTogaW1wb3J0KFwiY2xpcGFuaW9uXCIpLlVzYWdlO1xuICAgIGN3ZDogc3RyaW5nO1xuICAgIGNvbmZpZ1BhdGg/OiBzdHJpbmc7XG4gICAgcGFja2FnZUpzb25QYXRoOiBzdHJpbmc7XG4gICAgbnBtRGlyOiBzdHJpbmc7XG4gICAgZHJ5UnVuOiBib29sZWFuO1xuICAgIGdldE9wdGlvbnMoKToge1xuICAgICAgICBjd2Q6IHN0cmluZztcbiAgICAgICAgY29uZmlnUGF0aDogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgICAgICBwYWNrYWdlSnNvblBhdGg6IHN0cmluZztcbiAgICAgICAgbnBtRGlyOiBzdHJpbmc7XG4gICAgICAgIGRyeVJ1bjogYm9vbGVhbjtcbiAgICB9O1xufVxuLyoqXG4gKiBDcmVhdGUgbnBtIHBhY2thZ2UgZGlycyBmb3IgZGlmZmVyZW50IHBsYXRmb3Jtc1xuICovXG5leHBvcnQgaW50ZXJmYWNlIENyZWF0ZU5wbURpcnNPcHRpb25zIHtcbiAgICAvKipcbiAgICAgKiBUaGUgd29ya2luZyBkaXJlY3Rvcnkgb2Ygd2hlcmUgbmFwaSBjb21tYW5kIHdpbGwgYmUgZXhlY3V0ZWQgaW4sIGFsbCBvdGhlciBwYXRocyBvcHRpb25zIGFyZSByZWxhdGl2ZSB0byB0aGlzIHBhdGhcbiAgICAgKlxuICAgICAqIEBkZWZhdWx0IHByb2Nlc3MuY3dkKClcbiAgICAgKi9cbiAgICBjd2Q/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogUGF0aCB0byBgbmFwaWAgY29uZmlnIGpzb24gZmlsZVxuICAgICAqL1xuICAgIGNvbmZpZ1BhdGg/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogUGF0aCB0byBgcGFja2FnZS5qc29uYFxuICAgICAqXG4gICAgICogQGRlZmF1bHQgJ3BhY2thZ2UuanNvbidcbiAgICAgKi9cbiAgICBwYWNrYWdlSnNvblBhdGg/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogUGF0aCB0byB0aGUgZm9sZGVyIHdoZXJlIHRoZSBucG0gcGFja2FnZXMgcHV0XG4gICAgICpcbiAgICAgKiBAZGVmYXVsdCAnbnBtJ1xuICAgICAqL1xuICAgIG5wbURpcj86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBEcnkgcnVuIHdpdGhvdXQgdG91Y2hpbmcgZmlsZSBzeXN0ZW1cbiAgICAgKlxuICAgICAqIEBkZWZhdWx0IGZhbHNlXG4gICAgICovXG4gICAgZHJ5UnVuPzogYm9vbGVhbjtcbn1cbmV4cG9ydCBkZWNsYXJlIGZ1bmN0aW9uIGFwcGx5RGVmYXVsdENyZWF0ZU5wbURpcnNPcHRpb25zKG9wdGlvbnM6IENyZWF0ZU5wbURpcnNPcHRpb25zKToge1xuICAgIGN3ZDogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gYG5hcGlgIGNvbmZpZyBqc29uIGZpbGVcbiAgICAgKi9cbiAgICBjb25maWdQYXRoPzogc3RyaW5nO1xuICAgIHBhY2thZ2VKc29uUGF0aDogc3RyaW5nO1xuICAgIG5wbURpcjogc3RyaW5nO1xuICAgIGRyeVJ1bjogYm9vbGVhbjtcbn07XG4iLCJpbXBvcnQgeyB0eXBlIENyZWF0ZU5wbURpcnNPcHRpb25zIH0gZnJvbSAnLi4vZGVmL2NyZWF0ZS1ucG0tZGlycy5qcyc7XG5leHBvcnQgaW50ZXJmYWNlIFBhY2thZ2VNZXRhIHtcbiAgICAnZGlzdC10YWdzJzoge1xuICAgICAgICBbaW5kZXg6IHN0cmluZ106IHN0cmluZztcbiAgICB9O1xufVxuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gY3JlYXRlTnBtRGlycyh1c2VyT3B0aW9uczogQ3JlYXRlTnBtRGlyc09wdGlvbnMpOiBQcm9taXNlPHZvaWQ+O1xuIiwiaW1wb3J0IHsgQ29tbWFuZCB9IGZyb20gJ2NsaXBhbmlvbic7XG5leHBvcnQgZGVjbGFyZSBhYnN0cmFjdCBjbGFzcyBCYXNlTmV3Q29tbWFuZCBleHRlbmRzIENvbW1hbmQge1xuICAgIHN0YXRpYyBwYXRoczogc3RyaW5nW11bXTtcbiAgICBzdGF0aWMgdXNhZ2U6IGltcG9ydChcImNsaXBhbmlvblwiKS5Vc2FnZTtcbiAgICAkJHBhdGg6IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICAkJG5hbWU/OiBzdHJpbmc7XG4gICAgbWluTm9kZUFwaVZlcnNpb246IG51bWJlcjtcbiAgICBwYWNrYWdlTWFuYWdlcjogc3RyaW5nO1xuICAgIGxpY2Vuc2U6IHN0cmluZztcbiAgICB0YXJnZXRzOiBzdHJpbmdbXTtcbiAgICBlbmFibGVEZWZhdWx0VGFyZ2V0czogYm9vbGVhbjtcbiAgICBlbmFibGVBbGxUYXJnZXRzOiBib29sZWFuO1xuICAgIGVuYWJsZVR5cGVEZWY6IGJvb2xlYW47XG4gICAgZW5hYmxlR2l0aHViQWN0aW9uczogYm9vbGVhbjtcbiAgICB0ZXN0RnJhbWV3b3JrOiBzdHJpbmc7XG4gICAgZHJ5UnVuOiBib29sZWFuO1xuICAgIGdldE9wdGlvbnMoKToge1xuICAgICAgICBwYXRoOiBzdHJpbmcgfCB1bmRlZmluZWQ7XG4gICAgICAgIG5hbWU6IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICAgICAgbWluTm9kZUFwaVZlcnNpb246IG51bWJlcjtcbiAgICAgICAgcGFja2FnZU1hbmFnZXI6IHN0cmluZztcbiAgICAgICAgbGljZW5zZTogc3RyaW5nO1xuICAgICAgICB0YXJnZXRzOiBzdHJpbmdbXTtcbiAgICAgICAgZW5hYmxlRGVmYXVsdFRhcmdldHM6IGJvb2xlYW47XG4gICAgICAgIGVuYWJsZUFsbFRhcmdldHM6IGJvb2xlYW47XG4gICAgICAgIGVuYWJsZVR5cGVEZWY6IGJvb2xlYW47XG4gICAgICAgIGVuYWJsZUdpdGh1YkFjdGlvbnM6IGJvb2xlYW47XG4gICAgICAgIHRlc3RGcmFtZXdvcms6IHN0cmluZztcbiAgICAgICAgZHJ5UnVuOiBib29sZWFuO1xuICAgIH07XG59XG4vKipcbiAqIENyZWF0ZSBhIG5ldyBwcm9qZWN0IHdpdGggcHJlLWNvbmZpZ3VyZWQgYm9pbGVycGxhdGVcbiAqL1xuZXhwb3J0IGludGVyZmFjZSBOZXdPcHRpb25zIHtcbiAgICAvKipcbiAgICAgKiBUaGUgcGF0aCB3aGVyZSB0aGUgTkFQSS1SUyBwcm9qZWN0IHdpbGwgYmUgY3JlYXRlZC5cbiAgICAgKi9cbiAgICBwYXRoPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFRoZSBuYW1lIG9mIHRoZSBwcm9qZWN0LCBkZWZhdWx0IHRvIHRoZSBuYW1lIG9mIHRoZSBkaXJlY3RvcnkgaWYgbm90IHByb3ZpZGVkXG4gICAgICovXG4gICAgbmFtZT86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBUaGUgbWluaW11bSBOb2RlLUFQSSB2ZXJzaW9uIHRvIHN1cHBvcnRcbiAgICAgKlxuICAgICAqIEBkZWZhdWx0IDRcbiAgICAgKi9cbiAgICBtaW5Ob2RlQXBpVmVyc2lvbj86IG51bWJlcjtcbiAgICAvKipcbiAgICAgKiBUaGUgcGFja2FnZSBtYW5hZ2VyIHRvIHVzZS4gT25seSBzdXBwb3J0IHlhcm4gNC54IGZvciBub3cuXG4gICAgICpcbiAgICAgKiBAZGVmYXVsdCAneWFybidcbiAgICAgKi9cbiAgICBwYWNrYWdlTWFuYWdlcj86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBMaWNlbnNlIGZvciBvcGVuLXNvdXJjZWQgcHJvamVjdFxuICAgICAqXG4gICAgICogQGRlZmF1bHQgJ01JVCdcbiAgICAgKi9cbiAgICBsaWNlbnNlPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIEFsbCB0YXJnZXRzIHRoZSBjcmF0ZSB3aWxsIGJlIGNvbXBpbGVkIGZvci5cbiAgICAgKlxuICAgICAqIEBkZWZhdWx0IFtdXG4gICAgICovXG4gICAgdGFyZ2V0cz86IHN0cmluZ1tdO1xuICAgIC8qKlxuICAgICAqIFdoZXRoZXIgZW5hYmxlIGRlZmF1bHQgdGFyZ2V0c1xuICAgICAqXG4gICAgICogQGRlZmF1bHQgdHJ1ZVxuICAgICAqL1xuICAgIGVuYWJsZURlZmF1bHRUYXJnZXRzPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBXaGV0aGVyIGVuYWJsZSBhbGwgdGFyZ2V0c1xuICAgICAqXG4gICAgICogQGRlZmF1bHQgZmFsc2VcbiAgICAgKi9cbiAgICBlbmFibGVBbGxUYXJnZXRzPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBXaGV0aGVyIGVuYWJsZSB0aGUgYHR5cGUtZGVmYCBmZWF0dXJlIGZvciB0eXBlc2NyaXB0IGRlZmluaXRpb25zIGF1dG8tZ2VuZXJhdGlvblxuICAgICAqXG4gICAgICogQGRlZmF1bHQgdHJ1ZVxuICAgICAqL1xuICAgIGVuYWJsZVR5cGVEZWY/OiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIFdoZXRoZXIgZ2VuZXJhdGUgcHJlY29uZmlndXJlZCBHaXRIdWIgQWN0aW9ucyB3b3JrZmxvd1xuICAgICAqXG4gICAgICogQGRlZmF1bHQgdHJ1ZVxuICAgICAqL1xuICAgIGVuYWJsZUdpdGh1YkFjdGlvbnM/OiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIFRoZSBKYXZhU2NyaXB0IHRlc3QgZnJhbWV3b3JrIHRvIHVzZSwgb25seSBzdXBwb3J0IGBhdmFgIGZvciBub3dcbiAgICAgKlxuICAgICAqIEBkZWZhdWx0ICdhdmEnXG4gICAgICovXG4gICAgdGVzdEZyYW1ld29yaz86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBXaGV0aGVyIHRvIHJ1biB0aGUgY29tbWFuZCBpbiBkcnktcnVuIG1vZGVcbiAgICAgKlxuICAgICAqIEBkZWZhdWx0IGZhbHNlXG4gICAgICovXG4gICAgZHJ5UnVuPzogYm9vbGVhbjtcbn1cbmV4cG9ydCBkZWNsYXJlIGZ1bmN0aW9uIGFwcGx5RGVmYXVsdE5ld09wdGlvbnMob3B0aW9uczogTmV3T3B0aW9ucyk6IHtcbiAgICAvKipcbiAgICAgKiBUaGUgcGF0aCB3aGVyZSB0aGUgTkFQSS1SUyBwcm9qZWN0IHdpbGwgYmUgY3JlYXRlZC5cbiAgICAgKi9cbiAgICBwYXRoPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFRoZSBuYW1lIG9mIHRoZSBwcm9qZWN0LCBkZWZhdWx0IHRvIHRoZSBuYW1lIG9mIHRoZSBkaXJlY3RvcnkgaWYgbm90IHByb3ZpZGVkXG4gICAgICovXG4gICAgbmFtZT86IHN0cmluZztcbiAgICBtaW5Ob2RlQXBpVmVyc2lvbjogbnVtYmVyO1xuICAgIHBhY2thZ2VNYW5hZ2VyOiBzdHJpbmc7XG4gICAgbGljZW5zZTogc3RyaW5nO1xuICAgIHRhcmdldHM6IHN0cmluZ1tdO1xuICAgIGVuYWJsZURlZmF1bHRUYXJnZXRzOiBib29sZWFuO1xuICAgIGVuYWJsZUFsbFRhcmdldHM6IGJvb2xlYW47XG4gICAgZW5hYmxlVHlwZURlZjogYm9vbGVhbjtcbiAgICBlbmFibGVHaXRodWJBY3Rpb25zOiBib29sZWFuO1xuICAgIHRlc3RGcmFtZXdvcms6IHN0cmluZztcbiAgICBkcnlSdW46IGJvb2xlYW47XG59O1xuIiwiaW1wb3J0IHsgdHlwZSBOZXdPcHRpb25zIGFzIFJhd05ld09wdGlvbnMgfSBmcm9tICcuLi9kZWYvbmV3LmpzJztcbnR5cGUgTmV3T3B0aW9ucyA9IFJlcXVpcmVkPFJhd05ld09wdGlvbnM+O1xuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gbmV3UHJvamVjdCh1c2VyT3B0aW9uczogUmF3TmV3T3B0aW9ucyk6IFByb21pc2U8dm9pZD47XG5leHBvcnQgdHlwZSB7IE5ld09wdGlvbnMgfTtcbiIsImltcG9ydCB7IENvbW1hbmQgfSBmcm9tICdjbGlwYW5pb24nO1xuZXhwb3J0IGRlY2xhcmUgYWJzdHJhY3QgY2xhc3MgQmFzZVByZVB1Ymxpc2hDb21tYW5kIGV4dGVuZHMgQ29tbWFuZCB7XG4gICAgc3RhdGljIHBhdGhzOiBzdHJpbmdbXVtdO1xuICAgIHN0YXRpYyB1c2FnZTogaW1wb3J0KFwiY2xpcGFuaW9uXCIpLlVzYWdlO1xuICAgIGN3ZDogc3RyaW5nO1xuICAgIGNvbmZpZ1BhdGg/OiBzdHJpbmc7XG4gICAgcGFja2FnZUpzb25QYXRoOiBzdHJpbmc7XG4gICAgbnBtRGlyOiBzdHJpbmc7XG4gICAgdGFnU3R5bGU6IHN0cmluZztcbiAgICBnaFJlbGVhc2U6IGJvb2xlYW47XG4gICAgZ2hSZWxlYXNlTmFtZT86IHN0cmluZztcbiAgICBnaFJlbGVhc2VJZD86IHN0cmluZztcbiAgICBza2lwT3B0aW9uYWxQdWJsaXNoOiBib29sZWFuO1xuICAgIGRyeVJ1bjogYm9vbGVhbjtcbiAgICBnZXRPcHRpb25zKCk6IHtcbiAgICAgICAgY3dkOiBzdHJpbmc7XG4gICAgICAgIGNvbmZpZ1BhdGg6IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICAgICAgcGFja2FnZUpzb25QYXRoOiBzdHJpbmc7XG4gICAgICAgIG5wbURpcjogc3RyaW5nO1xuICAgICAgICB0YWdTdHlsZTogc3RyaW5nO1xuICAgICAgICBnaFJlbGVhc2U6IGJvb2xlYW47XG4gICAgICAgIGdoUmVsZWFzZU5hbWU6IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICAgICAgZ2hSZWxlYXNlSWQ6IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICAgICAgc2tpcE9wdGlvbmFsUHVibGlzaDogYm9vbGVhbjtcbiAgICAgICAgZHJ5UnVuOiBib29sZWFuO1xuICAgIH07XG59XG4vKipcbiAqIFVwZGF0ZSBwYWNrYWdlLmpzb24gYW5kIGNvcHkgYWRkb25zIGludG8gcGVyIHBsYXRmb3JtIHBhY2thZ2VzXG4gKi9cbmV4cG9ydCBpbnRlcmZhY2UgUHJlUHVibGlzaE9wdGlvbnMge1xuICAgIC8qKlxuICAgICAqIFRoZSB3b3JraW5nIGRpcmVjdG9yeSBvZiB3aGVyZSBuYXBpIGNvbW1hbmQgd2lsbCBiZSBleGVjdXRlZCBpbiwgYWxsIG90aGVyIHBhdGhzIG9wdGlvbnMgYXJlIHJlbGF0aXZlIHRvIHRoaXMgcGF0aFxuICAgICAqXG4gICAgICogQGRlZmF1bHQgcHJvY2Vzcy5jd2QoKVxuICAgICAqL1xuICAgIGN3ZD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIGBuYXBpYCBjb25maWcganNvbiBmaWxlXG4gICAgICovXG4gICAgY29uZmlnUGF0aD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIGBwYWNrYWdlLmpzb25gXG4gICAgICpcbiAgICAgKiBAZGVmYXVsdCAncGFja2FnZS5qc29uJ1xuICAgICAqL1xuICAgIHBhY2thZ2VKc29uUGF0aD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIHRoZSBmb2xkZXIgd2hlcmUgdGhlIG5wbSBwYWNrYWdlcyBwdXRcbiAgICAgKlxuICAgICAqIEBkZWZhdWx0ICducG0nXG4gICAgICovXG4gICAgbnBtRGlyPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIGdpdCB0YWcgc3R5bGUsIGBucG1gIG9yIGBsZXJuYWBcbiAgICAgKlxuICAgICAqIEBkZWZhdWx0ICdsZXJuYSdcbiAgICAgKi9cbiAgICB0YWdTdHlsZT86ICducG0nIHwgJ2xlcm5hJztcbiAgICAvKipcbiAgICAgKiBXaGV0aGVyIGNyZWF0ZSBHaXRIdWIgcmVsZWFzZVxuICAgICAqXG4gICAgICogQGRlZmF1bHQgdHJ1ZVxuICAgICAqL1xuICAgIGdoUmVsZWFzZT86IGJvb2xlYW47XG4gICAgLyoqXG4gICAgICogR2l0SHViIHJlbGVhc2UgbmFtZVxuICAgICAqL1xuICAgIGdoUmVsZWFzZU5hbWU/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogRXhpc3RpbmcgR2l0SHViIHJlbGVhc2UgaWRcbiAgICAgKi9cbiAgICBnaFJlbGVhc2VJZD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBXaGV0aGVyIHNraXAgb3B0aW9uYWxEZXBlbmRlbmNpZXMgcGFja2FnZXMgcHVibGlzaFxuICAgICAqXG4gICAgICogQGRlZmF1bHQgZmFsc2VcbiAgICAgKi9cbiAgICBza2lwT3B0aW9uYWxQdWJsaXNoPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBEcnkgcnVuIHdpdGhvdXQgdG91Y2hpbmcgZmlsZSBzeXN0ZW1cbiAgICAgKlxuICAgICAqIEBkZWZhdWx0IGZhbHNlXG4gICAgICovXG4gICAgZHJ5UnVuPzogYm9vbGVhbjtcbn1cbmV4cG9ydCBkZWNsYXJlIGZ1bmN0aW9uIGFwcGx5RGVmYXVsdFByZVB1Ymxpc2hPcHRpb25zKG9wdGlvbnM6IFByZVB1Ymxpc2hPcHRpb25zKToge1xuICAgIGN3ZDogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gYG5hcGlgIGNvbmZpZyBqc29uIGZpbGVcbiAgICAgKi9cbiAgICBjb25maWdQYXRoPzogc3RyaW5nO1xuICAgIHBhY2thZ2VKc29uUGF0aDogc3RyaW5nO1xuICAgIG5wbURpcjogc3RyaW5nO1xuICAgIHRhZ1N0eWxlOiBzdHJpbmc7XG4gICAgZ2hSZWxlYXNlOiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIEdpdEh1YiByZWxlYXNlIG5hbWVcbiAgICAgKi9cbiAgICBnaFJlbGVhc2VOYW1lPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIEV4aXN0aW5nIEdpdEh1YiByZWxlYXNlIGlkXG4gICAgICovXG4gICAgZ2hSZWxlYXNlSWQ/OiBzdHJpbmc7XG4gICAgc2tpcE9wdGlvbmFsUHVibGlzaDogYm9vbGVhbjtcbiAgICBkcnlSdW46IGJvb2xlYW47XG59O1xuIiwiaW1wb3J0IHsgdHlwZSBQcmVQdWJsaXNoT3B0aW9ucyB9IGZyb20gJy4uL2RlZi9wcmUtcHVibGlzaC5qcyc7XG5leHBvcnQgZGVjbGFyZSBmdW5jdGlvbiBwcmVQdWJsaXNoKHVzZXJPcHRpb25zOiBQcmVQdWJsaXNoT3B0aW9ucyk6IFByb21pc2U8dm9pZD47XG4iLCJpbXBvcnQgeyBDb21tYW5kIH0gZnJvbSAnY2xpcGFuaW9uJztcbmV4cG9ydCBkZWNsYXJlIGFic3RyYWN0IGNsYXNzIEJhc2VSZW5hbWVDb21tYW5kIGV4dGVuZHMgQ29tbWFuZCB7XG4gICAgc3RhdGljIHBhdGhzOiBzdHJpbmdbXVtdO1xuICAgIHN0YXRpYyB1c2FnZTogaW1wb3J0KFwiY2xpcGFuaW9uXCIpLlVzYWdlO1xuICAgIGN3ZDogc3RyaW5nO1xuICAgIGNvbmZpZ1BhdGg/OiBzdHJpbmc7XG4gICAgcGFja2FnZUpzb25QYXRoOiBzdHJpbmc7XG4gICAgbnBtRGlyOiBzdHJpbmc7XG4gICAgJCRuYW1lPzogc3RyaW5nO1xuICAgIGJpbmFyeU5hbWU/OiBzdHJpbmc7XG4gICAgcGFja2FnZU5hbWU/OiBzdHJpbmc7XG4gICAgbWFuaWZlc3RQYXRoOiBzdHJpbmc7XG4gICAgcmVwb3NpdG9yeT86IHN0cmluZztcbiAgICBkZXNjcmlwdGlvbj86IHN0cmluZztcbiAgICBnZXRPcHRpb25zKCk6IHtcbiAgICAgICAgY3dkOiBzdHJpbmc7XG4gICAgICAgIGNvbmZpZ1BhdGg6IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICAgICAgcGFja2FnZUpzb25QYXRoOiBzdHJpbmc7XG4gICAgICAgIG5wbURpcjogc3RyaW5nO1xuICAgICAgICBuYW1lOiBzdHJpbmcgfCB1bmRlZmluZWQ7XG4gICAgICAgIGJpbmFyeU5hbWU6IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICAgICAgcGFja2FnZU5hbWU6IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICAgICAgbWFuaWZlc3RQYXRoOiBzdHJpbmc7XG4gICAgICAgIHJlcG9zaXRvcnk6IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICAgICAgZGVzY3JpcHRpb246IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICB9O1xufVxuLyoqXG4gKiBSZW5hbWUgdGhlIE5BUEktUlMgcHJvamVjdFxuICovXG5leHBvcnQgaW50ZXJmYWNlIFJlbmFtZU9wdGlvbnMge1xuICAgIC8qKlxuICAgICAqIFRoZSB3b3JraW5nIGRpcmVjdG9yeSBvZiB3aGVyZSBuYXBpIGNvbW1hbmQgd2lsbCBiZSBleGVjdXRlZCBpbiwgYWxsIG90aGVyIHBhdGhzIG9wdGlvbnMgYXJlIHJlbGF0aXZlIHRvIHRoaXMgcGF0aFxuICAgICAqXG4gICAgICogQGRlZmF1bHQgcHJvY2Vzcy5jd2QoKVxuICAgICAqL1xuICAgIGN3ZD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIGBuYXBpYCBjb25maWcganNvbiBmaWxlXG4gICAgICovXG4gICAgY29uZmlnUGF0aD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIGBwYWNrYWdlLmpzb25gXG4gICAgICpcbiAgICAgKiBAZGVmYXVsdCAncGFja2FnZS5qc29uJ1xuICAgICAqL1xuICAgIHBhY2thZ2VKc29uUGF0aD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIHRoZSBmb2xkZXIgd2hlcmUgdGhlIG5wbSBwYWNrYWdlcyBwdXRcbiAgICAgKlxuICAgICAqIEBkZWZhdWx0ICducG0nXG4gICAgICovXG4gICAgbnBtRGlyPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFRoZSBuZXcgbmFtZSBvZiB0aGUgcHJvamVjdFxuICAgICAqL1xuICAgIG5hbWU/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogVGhlIG5ldyBiaW5hcnkgbmFtZSAqLm5vZGUgZmlsZXNcbiAgICAgKi9cbiAgICBiaW5hcnlOYW1lPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFRoZSBuZXcgcGFja2FnZSBuYW1lIG9mIHRoZSBwcm9qZWN0XG4gICAgICovXG4gICAgcGFja2FnZU5hbWU/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogUGF0aCB0byBgQ2FyZ28udG9tbGBcbiAgICAgKlxuICAgICAqIEBkZWZhdWx0ICdDYXJnby50b21sJ1xuICAgICAqL1xuICAgIG1hbmlmZXN0UGF0aD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBUaGUgbmV3IHJlcG9zaXRvcnkgb2YgdGhlIHByb2plY3RcbiAgICAgKi9cbiAgICByZXBvc2l0b3J5Pzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFRoZSBuZXcgZGVzY3JpcHRpb24gb2YgdGhlIHByb2plY3RcbiAgICAgKi9cbiAgICBkZXNjcmlwdGlvbj86IHN0cmluZztcbn1cbmV4cG9ydCBkZWNsYXJlIGZ1bmN0aW9uIGFwcGx5RGVmYXVsdFJlbmFtZU9wdGlvbnMob3B0aW9uczogUmVuYW1lT3B0aW9ucyk6IHtcbiAgICBjd2Q6IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIGBuYXBpYCBjb25maWcganNvbiBmaWxlXG4gICAgICovXG4gICAgY29uZmlnUGF0aD86IHN0cmluZztcbiAgICBwYWNrYWdlSnNvblBhdGg6IHN0cmluZztcbiAgICBucG1EaXI6IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBUaGUgbmV3IG5hbWUgb2YgdGhlIHByb2plY3RcbiAgICAgKi9cbiAgICBuYW1lPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFRoZSBuZXcgYmluYXJ5IG5hbWUgKi5ub2RlIGZpbGVzXG4gICAgICovXG4gICAgYmluYXJ5TmFtZT86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBUaGUgbmV3IHBhY2thZ2UgbmFtZSBvZiB0aGUgcHJvamVjdFxuICAgICAqL1xuICAgIHBhY2thZ2VOYW1lPzogc3RyaW5nO1xuICAgIG1hbmlmZXN0UGF0aDogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFRoZSBuZXcgcmVwb3NpdG9yeSBvZiB0aGUgcHJvamVjdFxuICAgICAqL1xuICAgIHJlcG9zaXRvcnk/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogVGhlIG5ldyBkZXNjcmlwdGlvbiBvZiB0aGUgcHJvamVjdFxuICAgICAqL1xuICAgIGRlc2NyaXB0aW9uPzogc3RyaW5nO1xufTtcbiIsImltcG9ydCB7IHR5cGUgUmVuYW1lT3B0aW9ucyB9IGZyb20gJy4uL2RlZi9yZW5hbWUuanMnO1xuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gcmVuYW1lUHJvamVjdCh1c2VyT3B0aW9uczogUmVuYW1lT3B0aW9ucyk6IFByb21pc2U8dm9pZD47XG4iLCJpbXBvcnQgeyBDb21tYW5kIH0gZnJvbSAnY2xpcGFuaW9uJztcbmV4cG9ydCBkZWNsYXJlIGFic3RyYWN0IGNsYXNzIEJhc2VVbml2ZXJzYWxpemVDb21tYW5kIGV4dGVuZHMgQ29tbWFuZCB7XG4gICAgc3RhdGljIHBhdGhzOiBzdHJpbmdbXVtdO1xuICAgIHN0YXRpYyB1c2FnZTogaW1wb3J0KFwiY2xpcGFuaW9uXCIpLlVzYWdlO1xuICAgIGN3ZDogc3RyaW5nO1xuICAgIGNvbmZpZ1BhdGg/OiBzdHJpbmc7XG4gICAgcGFja2FnZUpzb25QYXRoOiBzdHJpbmc7XG4gICAgb3V0cHV0RGlyOiBzdHJpbmc7XG4gICAgZ2V0T3B0aW9ucygpOiB7XG4gICAgICAgIGN3ZDogc3RyaW5nO1xuICAgICAgICBjb25maWdQYXRoOiBzdHJpbmcgfCB1bmRlZmluZWQ7XG4gICAgICAgIHBhY2thZ2VKc29uUGF0aDogc3RyaW5nO1xuICAgICAgICBvdXRwdXREaXI6IHN0cmluZztcbiAgICB9O1xufVxuLyoqXG4gKiBDb21iaWxlIGJ1aWx0IGJpbmFyaWVzIGludG8gb25lIHVuaXZlcnNhbCBiaW5hcnlcbiAqL1xuZXhwb3J0IGludGVyZmFjZSBVbml2ZXJzYWxpemVPcHRpb25zIHtcbiAgICAvKipcbiAgICAgKiBUaGUgd29ya2luZyBkaXJlY3Rvcnkgb2Ygd2hlcmUgbmFwaSBjb21tYW5kIHdpbGwgYmUgZXhlY3V0ZWQgaW4sIGFsbCBvdGhlciBwYXRocyBvcHRpb25zIGFyZSByZWxhdGl2ZSB0byB0aGlzIHBhdGhcbiAgICAgKlxuICAgICAqIEBkZWZhdWx0IHByb2Nlc3MuY3dkKClcbiAgICAgKi9cbiAgICBjd2Q/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogUGF0aCB0byBgbmFwaWAgY29uZmlnIGpzb24gZmlsZVxuICAgICAqL1xuICAgIGNvbmZpZ1BhdGg/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogUGF0aCB0byBgcGFja2FnZS5qc29uYFxuICAgICAqXG4gICAgICogQGRlZmF1bHQgJ3BhY2thZ2UuanNvbidcbiAgICAgKi9cbiAgICBwYWNrYWdlSnNvblBhdGg/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogUGF0aCB0byB0aGUgZm9sZGVyIHdoZXJlIGFsbCBidWlsdCBgLm5vZGVgIGZpbGVzIHB1dCwgc2FtZSBhcyBgLS1vdXRwdXQtZGlyYCBvZiBidWlsZCBjb21tYW5kXG4gICAgICpcbiAgICAgKiBAZGVmYXVsdCAnLi8nXG4gICAgICovXG4gICAgb3V0cHV0RGlyPzogc3RyaW5nO1xufVxuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gYXBwbHlEZWZhdWx0VW5pdmVyc2FsaXplT3B0aW9ucyhvcHRpb25zOiBVbml2ZXJzYWxpemVPcHRpb25zKToge1xuICAgIGN3ZDogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gYG5hcGlgIGNvbmZpZyBqc29uIGZpbGVcbiAgICAgKi9cbiAgICBjb25maWdQYXRoPzogc3RyaW5nO1xuICAgIHBhY2thZ2VKc29uUGF0aDogc3RyaW5nO1xuICAgIG91dHB1dERpcjogc3RyaW5nO1xufTtcbiIsImltcG9ydCB7IHR5cGUgVW5pdmVyc2FsaXplT3B0aW9ucyB9IGZyb20gJy4uL2RlZi91bml2ZXJzYWxpemUuanMnO1xuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gdW5pdmVyc2FsaXplQmluYXJpZXModXNlck9wdGlvbnM6IFVuaXZlcnNhbGl6ZU9wdGlvbnMpOiBQcm9taXNlPHZvaWQ+O1xuIiwiaW1wb3J0IHsgQ29tbWFuZCB9IGZyb20gJ2NsaXBhbmlvbic7XG5leHBvcnQgZGVjbGFyZSBhYnN0cmFjdCBjbGFzcyBCYXNlVmVyc2lvbkNvbW1hbmQgZXh0ZW5kcyBDb21tYW5kIHtcbiAgICBzdGF0aWMgcGF0aHM6IHN0cmluZ1tdW107XG4gICAgc3RhdGljIHVzYWdlOiBpbXBvcnQoXCJjbGlwYW5pb25cIikuVXNhZ2U7XG4gICAgY3dkOiBzdHJpbmc7XG4gICAgY29uZmlnUGF0aD86IHN0cmluZztcbiAgICBwYWNrYWdlSnNvblBhdGg6IHN0cmluZztcbiAgICBucG1EaXI6IHN0cmluZztcbiAgICBnZXRPcHRpb25zKCk6IHtcbiAgICAgICAgY3dkOiBzdHJpbmc7XG4gICAgICAgIGNvbmZpZ1BhdGg6IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICAgICAgcGFja2FnZUpzb25QYXRoOiBzdHJpbmc7XG4gICAgICAgIG5wbURpcjogc3RyaW5nO1xuICAgIH07XG59XG4vKipcbiAqIFVwZGF0ZSB2ZXJzaW9uIGluIGNyZWF0ZWQgbnBtIHBhY2thZ2VzXG4gKi9cbmV4cG9ydCBpbnRlcmZhY2UgVmVyc2lvbk9wdGlvbnMge1xuICAgIC8qKlxuICAgICAqIFRoZSB3b3JraW5nIGRpcmVjdG9yeSBvZiB3aGVyZSBuYXBpIGNvbW1hbmQgd2lsbCBiZSBleGVjdXRlZCBpbiwgYWxsIG90aGVyIHBhdGhzIG9wdGlvbnMgYXJlIHJlbGF0aXZlIHRvIHRoaXMgcGF0aFxuICAgICAqXG4gICAgICogQGRlZmF1bHQgcHJvY2Vzcy5jd2QoKVxuICAgICAqL1xuICAgIGN3ZD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIGBuYXBpYCBjb25maWcganNvbiBmaWxlXG4gICAgICovXG4gICAgY29uZmlnUGF0aD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIGBwYWNrYWdlLmpzb25gXG4gICAgICpcbiAgICAgKiBAZGVmYXVsdCAncGFja2FnZS5qc29uJ1xuICAgICAqL1xuICAgIHBhY2thZ2VKc29uUGF0aD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIHRoZSBmb2xkZXIgd2hlcmUgdGhlIG5wbSBwYWNrYWdlcyBwdXRcbiAgICAgKlxuICAgICAqIEBkZWZhdWx0ICducG0nXG4gICAgICovXG4gICAgbnBtRGlyPzogc3RyaW5nO1xufVxuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gYXBwbHlEZWZhdWx0VmVyc2lvbk9wdGlvbnMob3B0aW9uczogVmVyc2lvbk9wdGlvbnMpOiB7XG4gICAgY3dkOiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogUGF0aCB0byBgbmFwaWAgY29uZmlnIGpzb24gZmlsZVxuICAgICAqL1xuICAgIGNvbmZpZ1BhdGg/OiBzdHJpbmc7XG4gICAgcGFja2FnZUpzb25QYXRoOiBzdHJpbmc7XG4gICAgbnBtRGlyOiBzdHJpbmc7XG59O1xuIiwiaW1wb3J0IHsgdHlwZSBWZXJzaW9uT3B0aW9ucyB9IGZyb20gJy4uL2RlZi92ZXJzaW9uLmpzJztcbmV4cG9ydCBkZWNsYXJlIGZ1bmN0aW9uIHZlcnNpb24odXNlck9wdGlvbnM6IFZlcnNpb25PcHRpb25zKTogUHJvbWlzZTx2b2lkPjtcbiIsImltcG9ydCB7IEJhc2VBcnRpZmFjdHNDb21tYW5kIH0gZnJvbSAnLi4vZGVmL2FydGlmYWN0cy5qcyc7XG5leHBvcnQgZGVjbGFyZSBjbGFzcyBBcnRpZmFjdHNDb21tYW5kIGV4dGVuZHMgQmFzZUFydGlmYWN0c0NvbW1hbmQge1xuICAgIHN0YXRpYyB1c2FnZTogaW1wb3J0KFwiY2xpcGFuaW9uXCIpLlVzYWdlO1xuICAgIHN0YXRpYyBwYXRoczogc3RyaW5nW11bXTtcbiAgICBleGVjdXRlKCk6IFByb21pc2U8dm9pZD47XG59XG4iLCJpbXBvcnQgeyBCYXNlQnVpbGRDb21tYW5kIH0gZnJvbSAnLi4vZGVmL2J1aWxkLmpzJztcbmV4cG9ydCBkZWNsYXJlIGNsYXNzIEJ1aWxkQ29tbWFuZCBleHRlbmRzIEJhc2VCdWlsZENvbW1hbmQge1xuICAgIHBpcGU6IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICBjYXJnb09wdGlvbnM6IHN0cmluZ1tdO1xuICAgIGV4ZWN1dGUoKTogUHJvbWlzZTx2b2lkPjtcbn1cbiIsImltcG9ydCB7IEJhc2VDcmVhdGVOcG1EaXJzQ29tbWFuZCB9IGZyb20gJy4uL2RlZi9jcmVhdGUtbnBtLWRpcnMuanMnO1xuZXhwb3J0IGRlY2xhcmUgY2xhc3MgQ3JlYXRlTnBtRGlyc0NvbW1hbmQgZXh0ZW5kcyBCYXNlQ3JlYXRlTnBtRGlyc0NvbW1hbmQge1xuICAgIGV4ZWN1dGUoKTogUHJvbWlzZTx2b2lkPjtcbn1cbiIsImltcG9ydCB7IEJhc2VOZXdDb21tYW5kIH0gZnJvbSAnLi4vZGVmL25ldy5qcyc7XG5leHBvcnQgZGVjbGFyZSBjbGFzcyBOZXdDb21tYW5kIGV4dGVuZHMgQmFzZU5ld0NvbW1hbmQge1xuICAgIGludGVyYWN0aXZlOiBib29sZWFuO1xuICAgIGV4ZWN1dGUoKTogUHJvbWlzZTwwIHwgMT47XG4gICAgcHJpdmF0ZSBmZXRjaE9wdGlvbnM7XG4gICAgcHJpdmF0ZSBmZXRjaE5hbWU7XG4gICAgcHJpdmF0ZSBmZXRjaExpY2Vuc2U7XG4gICAgcHJpdmF0ZSBmZXRjaE5hcGlWZXJzaW9uO1xuICAgIHByaXZhdGUgZmV0Y2hUYXJnZXRzO1xuICAgIHByaXZhdGUgZmV0Y2hUeXBlRGVmO1xuICAgIHByaXZhdGUgZmV0Y2hHaXRodWJBY3Rpb25zO1xufVxuIiwiaW1wb3J0IHsgQmFzZVByZVB1Ymxpc2hDb21tYW5kIH0gZnJvbSAnLi4vZGVmL3ByZS1wdWJsaXNoLmpzJztcbmV4cG9ydCBkZWNsYXJlIGNsYXNzIFByZVB1Ymxpc2hDb21tYW5kIGV4dGVuZHMgQmFzZVByZVB1Ymxpc2hDb21tYW5kIHtcbiAgICBleGVjdXRlKCk6IFByb21pc2U8dm9pZD47XG59XG4iLCJpbXBvcnQgeyBCYXNlUmVuYW1lQ29tbWFuZCB9IGZyb20gJy4uL2RlZi9yZW5hbWUuanMnO1xuZXhwb3J0IGRlY2xhcmUgY2xhc3MgUmVuYW1lQ29tbWFuZCBleHRlbmRzIEJhc2VSZW5hbWVDb21tYW5kIHtcbiAgICBleGVjdXRlKCk6IFByb21pc2U8dm9pZD47XG59XG4iLCJpbXBvcnQgeyBCYXNlVW5pdmVyc2FsaXplQ29tbWFuZCB9IGZyb20gJy4uL2RlZi91bml2ZXJzYWxpemUuanMnO1xuZXhwb3J0IGRlY2xhcmUgY2xhc3MgVW5pdmVyc2FsaXplQ29tbWFuZCBleHRlbmRzIEJhc2VVbml2ZXJzYWxpemVDb21tYW5kIHtcbiAgICBleGVjdXRlKCk6IFByb21pc2U8dm9pZD47XG59XG4iLCJpbXBvcnQgeyBCYXNlVmVyc2lvbkNvbW1hbmQgfSBmcm9tICcuLi9kZWYvdmVyc2lvbi5qcyc7XG5leHBvcnQgZGVjbGFyZSBjbGFzcyBWZXJzaW9uQ29tbWFuZCBleHRlbmRzIEJhc2VWZXJzaW9uQ29tbWFuZCB7XG4gICAgZXhlY3V0ZSgpOiBQcm9taXNlPHZvaWQ+O1xufVxuIiwiZXhwb3J0IHR5cGUgUGxhdGZvcm0gPSBOb2RlSlMuUGxhdGZvcm0gfCAnd2FzbScgfCAnd2FzaScgfCAnb3Blbmhhcm1vbnknO1xuZXhwb3J0IGRlY2xhcmUgY29uc3QgVU5JVkVSU0FMX1RBUkdFVFM6IHtcbiAgICByZWFkb25seSAndW5pdmVyc2FsLWFwcGxlLWRhcndpbic6IHJlYWRvbmx5IFtcImFhcmNoNjQtYXBwbGUtZGFyd2luXCIsIFwieDg2XzY0LWFwcGxlLWRhcndpblwiXTtcbn07XG5leHBvcnQgZGVjbGFyZSBjb25zdCBBVkFJTEFCTEVfVEFSR0VUUzogcmVhZG9ubHkgW1wiYWFyY2g2NC1hcHBsZS1kYXJ3aW5cIiwgXCJhYXJjaDY0LWxpbnV4LWFuZHJvaWRcIiwgXCJhYXJjaDY0LXVua25vd24tbGludXgtZ251XCIsIFwiYWFyY2g2NC11bmtub3duLWxpbnV4LW11c2xcIiwgXCJhYXJjaDY0LXVua25vd24tbGludXgtb2hvc1wiLCBcImFhcmNoNjQtcGMtd2luZG93cy1tc3ZjXCIsIFwieDg2XzY0LWFwcGxlLWRhcndpblwiLCBcIng4Nl82NC1wYy13aW5kb3dzLW1zdmNcIiwgXCJ4ODZfNjQtcGMtd2luZG93cy1nbnVcIiwgXCJ4ODZfNjQtdW5rbm93bi1saW51eC1nbnVcIiwgXCJ4ODZfNjQtdW5rbm93bi1saW51eC1tdXNsXCIsIFwieDg2XzY0LXVua25vd24tbGludXgtb2hvc1wiLCBcIng4Nl82NC11bmtub3duLWZyZWVic2RcIiwgXCJpNjg2LXBjLXdpbmRvd3MtbXN2Y1wiLCBcImFybXY3LXVua25vd24tbGludXgtZ251ZWFiaWhmXCIsIFwiYXJtdjctdW5rbm93bi1saW51eC1tdXNsZWFiaWhmXCIsIFwiYXJtdjctbGludXgtYW5kcm9pZGVhYmlcIiwgXCJ1bml2ZXJzYWwtYXBwbGUtZGFyd2luXCIsIFwibG9vbmdhcmNoNjQtdW5rbm93bi1saW51eC1nbnVcIiwgXCJyaXNjdjY0Z2MtdW5rbm93bi1saW51eC1nbnVcIiwgXCJwb3dlcnBjNjRsZS11bmtub3duLWxpbnV4LWdudVwiLCBcInMzOTB4LXVua25vd24tbGludXgtZ251XCIsIFwid2FzbTMyLXdhc2ktcHJldmlldzEtdGhyZWFkc1wiLCBcIndhc20zMi13YXNpcDEtdGhyZWFkc1wiXTtcbmV4cG9ydCB0eXBlIFRhcmdldFRyaXBsZSA9ICh0eXBlb2YgQVZBSUxBQkxFX1RBUkdFVFMpW251bWJlcl07XG5leHBvcnQgZGVjbGFyZSBjb25zdCBERUZBVUxUX1RBUkdFVFM6IHJlYWRvbmx5IFtcIng4Nl82NC1hcHBsZS1kYXJ3aW5cIiwgXCJhYXJjaDY0LWFwcGxlLWRhcndpblwiLCBcIng4Nl82NC1wYy13aW5kb3dzLW1zdmNcIiwgXCJ4ODZfNjQtdW5rbm93bi1saW51eC1nbnVcIl07XG5leHBvcnQgZGVjbGFyZSBjb25zdCBUQVJHRVRfTElOS0VSOiBSZWNvcmQ8c3RyaW5nLCBzdHJpbmc+O1xudHlwZSBOb2RlSlNBcmNoID0gJ2FybScgfCAnYXJtNjQnIHwgJ2lhMzInIHwgJ2xvb25nNjQnIHwgJ21pcHMnIHwgJ21pcHNlbCcgfCAncHBjJyB8ICdwcGM2NCcgfCAncmlzY3Y2NCcgfCAnczM5MCcgfCAnczM5MHgnIHwgJ3gzMicgfCAneDY0JyB8ICd1bml2ZXJzYWwnIHwgJ3dhc20zMic7XG5leHBvcnQgZGVjbGFyZSBjb25zdCBOb2RlQXJjaFRvQ3B1OiBSZWNvcmQ8c3RyaW5nLCBzdHJpbmc+O1xuZXhwb3J0IGRlY2xhcmUgY29uc3QgVW5pQXJjaHNCeVBsYXRmb3JtOiBQYXJ0aWFsPFJlY29yZDxQbGF0Zm9ybSwgTm9kZUpTQXJjaFtdPj47XG5leHBvcnQgaW50ZXJmYWNlIFRhcmdldCB7XG4gICAgdHJpcGxlOiBzdHJpbmc7XG4gICAgcGxhdGZvcm1BcmNoQUJJOiBzdHJpbmc7XG4gICAgcGxhdGZvcm06IFBsYXRmb3JtO1xuICAgIGFyY2g6IE5vZGVKU0FyY2g7XG4gICAgYWJpOiBzdHJpbmcgfCBudWxsO1xufVxuLyoqXG4gKiBBIHRyaXBsZSBpcyBhIHNwZWNpZmljIGZvcm1hdCBmb3Igc3BlY2lmeWluZyBhIHRhcmdldCBhcmNoaXRlY3R1cmUuXG4gKiBUcmlwbGVzIG1heSBiZSByZWZlcnJlZCB0byBhcyBhIHRhcmdldCB0cmlwbGUgd2hpY2ggaXMgdGhlIGFyY2hpdGVjdHVyZSBmb3IgdGhlIGFydGlmYWN0IHByb2R1Y2VkLCBhbmQgdGhlIGhvc3QgdHJpcGxlIHdoaWNoIGlzIHRoZSBhcmNoaXRlY3R1cmUgdGhhdCB0aGUgY29tcGlsZXIgaXMgcnVubmluZyBvbi5cbiAqIFRoZSBnZW5lcmFsIGZvcm1hdCBvZiB0aGUgdHJpcGxlIGlzIGA8YXJjaD48c3ViPi08dmVuZG9yPi08c3lzPi08YWJpPmAgd2hlcmU6XG4gKiAgIC0gYGFyY2hgID0gVGhlIGJhc2UgQ1BVIGFyY2hpdGVjdHVyZSwgZm9yIGV4YW1wbGUgYHg4Nl82NGAsIGBpNjg2YCwgYGFybWAsIGB0aHVtYmAsIGBtaXBzYCwgZXRjLlxuICogICAtIGBzdWJgID0gVGhlIENQVSBzdWItYXJjaGl0ZWN0dXJlLCBmb3IgZXhhbXBsZSBgYXJtYCBoYXMgYHY3YCwgYHY3c2AsIGB2NXRlYCwgZXRjLlxuICogICAtIGB2ZW5kb3JgID0gVGhlIHZlbmRvciwgZm9yIGV4YW1wbGUgYHVua25vd25gLCBgYXBwbGVgLCBgcGNgLCBgbnZpZGlhYCwgZXRjLlxuICogICAtIGBzeXNgID0gVGhlIHN5c3RlbSBuYW1lLCBmb3IgZXhhbXBsZSBgbGludXhgLCBgd2luZG93c2AsIGBkYXJ3aW5gLCBldGMuIG5vbmUgaXMgdHlwaWNhbGx5IHVzZWQgZm9yIGJhcmUtbWV0YWwgd2l0aG91dCBhbiBPUy5cbiAqICAgLSBgYWJpYCA9IFRoZSBBQkksIGZvciBleGFtcGxlIGBnbnVgLCBgYW5kcm9pZGAsIGBlYWJpYCwgZXRjLlxuICovXG5leHBvcnQgZGVjbGFyZSBmdW5jdGlvbiBwYXJzZVRyaXBsZShyYXdUcmlwbGU6IHN0cmluZyk6IFRhcmdldDtcbmV4cG9ydCBkZWNsYXJlIGZ1bmN0aW9uIGdldFN5c3RlbURlZmF1bHRUYXJnZXQoKTogVGFyZ2V0O1xuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gZ2V0VGFyZ2V0TGlua2VyKHRhcmdldDogc3RyaW5nKTogc3RyaW5nIHwgdW5kZWZpbmVkO1xuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gdGFyZ2V0VG9FbnZWYXIodGFyZ2V0OiBzdHJpbmcpOiBzdHJpbmc7XG5leHBvcnQge307XG4iLCJpbXBvcnQgeyB0eXBlIFRhcmdldCB9IGZyb20gJy4vdGFyZ2V0LmpzJztcbmV4cG9ydCB0eXBlIFZhbHVlT2ZDb25zdEFycmF5PFQ+ID0gVFtFeGNsdWRlPGtleW9mIFQsIGtleW9mIEFycmF5PGFueT4+XTtcbmV4cG9ydCBkZWNsYXJlIGNvbnN0IFN1cHBvcnRlZFBhY2thZ2VNYW5hZ2VyczogcmVhZG9ubHkgW1wieWFyblwiLCBcInBucG1cIl07XG5leHBvcnQgZGVjbGFyZSBjb25zdCBTdXBwb3J0ZWRUZXN0RnJhbWV3b3JrczogcmVhZG9ubHkgW1wiYXZhXCJdO1xuZXhwb3J0IHR5cGUgU3VwcG9ydGVkUGFja2FnZU1hbmFnZXIgPSBWYWx1ZU9mQ29uc3RBcnJheTx0eXBlb2YgU3VwcG9ydGVkUGFja2FnZU1hbmFnZXJzPjtcbmV4cG9ydCB0eXBlIFN1cHBvcnRlZFRlc3RGcmFtZXdvcmsgPSBWYWx1ZU9mQ29uc3RBcnJheTx0eXBlb2YgU3VwcG9ydGVkVGVzdEZyYW1ld29ya3M+O1xuZXhwb3J0IGludGVyZmFjZSBVc2VyTmFwaUNvbmZpZyB7XG4gICAgLyoqXG4gICAgICogTmFtZSBvZiB0aGUgYmluYXJ5IHRvIGJlIGdlbmVyYXRlZCwgZGVmYXVsdCB0byBgaW5kZXhgXG4gICAgICovXG4gICAgYmluYXJ5TmFtZT86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBOYW1lIG9mIHRoZSBucG0gcGFja2FnZSwgZGVmYXVsdCB0byB0aGUgbmFtZSBvZiByb290IHBhY2thZ2UuanNvbiBuYW1lXG4gICAgICpcbiAgICAgKiBBbHdheXMgZ2l2ZW4gYEBzY29wZS9wa2dgIGFuZCBhcmNoIHN1ZmZpeCB3aWxsIGJlIGFwcGVuZGVkIGxpa2UgYEBzY29wZS9wa2ctbGludXgtZ251LXg2NGBcbiAgICAgKi9cbiAgICBwYWNrYWdlTmFtZT86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBBbGwgdGFyZ2V0cyB0aGUgY3JhdGUgd2lsbCBiZSBjb21waWxlZCBmb3JcbiAgICAgKi9cbiAgICB0YXJnZXRzPzogc3RyaW5nW107XG4gICAgLyoqXG4gICAgICogVGhlIG5wbSBjbGllbnQgcHJvamVjdCB1c2VzLlxuICAgICAqL1xuICAgIG5wbUNsaWVudD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBXaGV0aGVyIGdlbmVyYXRlIGNvbnN0IGVudW0gZm9yIHR5cGVzY3JpcHQgYmluZGluZ3NcbiAgICAgKi9cbiAgICBjb25zdEVudW0/OiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIGR0cyBoZWFkZXIgcHJlcGVuZCB0byB0aGUgZ2VuZXJhdGVkIGR0cyBmaWxlXG4gICAgICovXG4gICAgZHRzSGVhZGVyPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIGR0cyBoZWFkZXIgZmlsZSBwYXRoIHRvIGJlIHByZXBlbmRlZCB0byB0aGUgZ2VuZXJhdGVkIGR0cyBmaWxlXG4gICAgICogaWYgYm90aCBkdHNIZWFkZXIgYW5kIGR0c0hlYWRlckZpbGUgYXJlIHByb3ZpZGVkLCBkdHNIZWFkZXJGaWxlIHdpbGwgYmUgdXNlZFxuICAgICAqL1xuICAgIGR0c0hlYWRlckZpbGU/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogd2FzbSBjb21waWxhdGlvbiBvcHRpb25zXG4gICAgICovXG4gICAgd2FzbT86IHtcbiAgICAgICAgLyoqXG4gICAgICAgICAqIGh0dHBzOi8vZGV2ZWxvcGVyLm1vemlsbGEub3JnL2VuLVVTL2RvY3MvV2ViQXNzZW1ibHkvSmF2YVNjcmlwdF9pbnRlcmZhY2UvTWVtb3J5XG4gICAgICAgICAqIEBkZWZhdWx0IDQwMDAgcGFnZXMgKDI1Nk1pQilcbiAgICAgICAgICovXG4gICAgICAgIGluaXRpYWxNZW1vcnk/OiBudW1iZXI7XG4gICAgICAgIC8qKlxuICAgICAgICAgKiBAZGVmYXVsdCA2NTUzNiBwYWdlcyAoNEdpQilcbiAgICAgICAgICovXG4gICAgICAgIG1heGltdW1NZW1vcnk/OiBudW1iZXI7XG4gICAgICAgIC8qKlxuICAgICAgICAgKiBCcm93c2VyIHdhc20gYmluZGluZyBjb25maWd1cmF0aW9uXG4gICAgICAgICAqL1xuICAgICAgICBicm93c2VyOiB7XG4gICAgICAgICAgICAvKipcbiAgICAgICAgICAgICAqIFdoZXRoZXIgdG8gdXNlIGZzIG1vZHVsZSBpbiBicm93c2VyXG4gICAgICAgICAgICAgKi9cbiAgICAgICAgICAgIGZzPzogYm9vbGVhbjtcbiAgICAgICAgICAgIC8qKlxuICAgICAgICAgICAgICogV2hldGhlciB0byBpbml0aWFsaXplIHdhc20gYXN5bmNocm9ub3VzbHlcbiAgICAgICAgICAgICAqL1xuICAgICAgICAgICAgYXN5bmNJbml0PzogYm9vbGVhbjtcbiAgICAgICAgICAgIC8qKlxuICAgICAgICAgICAgICogV2hldGhlciB0byBpbmplY3QgYGJ1ZmZlcmAgdG8gZW1uYXBpIGNvbnRleHRcbiAgICAgICAgICAgICAqL1xuICAgICAgICAgICAgYnVmZmVyPzogYm9vbGVhbjtcbiAgICAgICAgfTtcbiAgICB9O1xuICAgIC8qKlxuICAgICAqIEBkZXByZWNhdGVkIGJpbmFyeU5hbWUgaW5zdGVhZFxuICAgICAqL1xuICAgIG5hbWU/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogQGRlcHJlY2F0ZWQgdXNlIHBhY2thZ2VOYW1lIGluc3RlYWRcbiAgICAgKi9cbiAgICBwYWNrYWdlPzoge1xuICAgICAgICBuYW1lPzogc3RyaW5nO1xuICAgIH07XG4gICAgLyoqXG4gICAgICogQGRlcHJlY2F0ZWQgdXNlIHRhcmdldHMgaW5zdGVhZFxuICAgICAqL1xuICAgIHRyaXBsZXM/OiB7XG4gICAgICAgIC8qKlxuICAgICAgICAgKiBXaGV0aGVyIGVuYWJsZSBkZWZhdWx0IHRhcmdldHNcbiAgICAgICAgICovXG4gICAgICAgIGRlZmF1bHRzOiBib29sZWFuO1xuICAgICAgICAvKipcbiAgICAgICAgICogQWRkaXRpb25hbCB0YXJnZXRzIHRvIGJlIGNvbXBpbGVkIGZvclxuICAgICAgICAgKi9cbiAgICAgICAgYWRkaXRpb25hbD86IHN0cmluZ1tdO1xuICAgIH07XG59XG5leHBvcnQgaW50ZXJmYWNlIENvbW1vblBhY2thZ2VKc29uRmllbGRzIHtcbiAgICBuYW1lOiBzdHJpbmc7XG4gICAgdmVyc2lvbjogc3RyaW5nO1xuICAgIGRlc2NyaXB0aW9uPzogc3RyaW5nO1xuICAgIGtleXdvcmRzPzogc3RyaW5nW107XG4gICAgYXV0aG9yPzogc3RyaW5nO1xuICAgIGF1dGhvcnM/OiBzdHJpbmdbXTtcbiAgICBsaWNlbnNlPzogc3RyaW5nO1xuICAgIGNwdT86IHN0cmluZ1tdO1xuICAgIG9zPzogc3RyaW5nW107XG4gICAgbGliYz86IHN0cmluZ1tdO1xuICAgIGZpbGVzPzogc3RyaW5nW107XG4gICAgcmVwb3NpdG9yeT86IGFueTtcbiAgICBob21lcGFnZT86IGFueTtcbiAgICBlbmdpbmVzPzogUmVjb3JkPHN0cmluZywgc3RyaW5nPjtcbiAgICBwdWJsaXNoQ29uZmlnPzogYW55O1xuICAgIGJ1Z3M/OiBhbnk7XG4gICAgbmFwaT86IFVzZXJOYXBpQ29uZmlnO1xuICAgIHR5cGU/OiAnbW9kdWxlJyB8ICdjb21tb25qcyc7XG4gICAgc2NyaXB0cz86IFJlY29yZDxzdHJpbmcsIHN0cmluZz47XG4gICAgbWFpbj86IHN0cmluZztcbiAgICBtb2R1bGU/OiBzdHJpbmc7XG4gICAgdHlwZXM/OiBzdHJpbmc7XG4gICAgYnJvd3Nlcj86IHN0cmluZztcbiAgICBleHBvcnRzPzogYW55O1xuICAgIGRlcGVuZGVuY2llcz86IFJlY29yZDxzdHJpbmcsIHN0cmluZz47XG4gICAgZGV2RGVwZW5kZW5jaWVzPzogUmVjb3JkPHN0cmluZywgc3RyaW5nPjtcbiAgICBhdmE/OiB7XG4gICAgICAgIHRpbWVvdXQ/OiBzdHJpbmc7XG4gICAgfTtcbn1cbmV4cG9ydCB0eXBlIE5hcGlDb25maWcgPSBSZXF1aXJlZDxQaWNrPFVzZXJOYXBpQ29uZmlnLCAnYmluYXJ5TmFtZScgfCAncGFja2FnZU5hbWUnIHwgJ25wbUNsaWVudCc+PiAmIFBpY2s8VXNlck5hcGlDb25maWcsICd3YXNtJyB8ICdkdHNIZWFkZXInIHwgJ2R0c0hlYWRlckZpbGUnIHwgJ2NvbnN0RW51bSc+ICYge1xuICAgIHRhcmdldHM6IFRhcmdldFtdO1xuICAgIHBhY2thZ2VKc29uOiBDb21tb25QYWNrYWdlSnNvbkZpZWxkcztcbn07XG5leHBvcnQgZGVjbGFyZSBmdW5jdGlvbiByZWFkTmFwaUNvbmZpZyhwYXRoOiBzdHJpbmcsIGNvbmZpZ1BhdGg/OiBzdHJpbmcpOiBQcm9taXNlPE5hcGlDb25maWc+O1xuIiwiaW1wb3J0IHsgQ2xpIH0gZnJvbSAnY2xpcGFuaW9uJztcbmltcG9ydCB7IGNvbGxlY3RBcnRpZmFjdHMgfSBmcm9tICcuL2FwaS9hcnRpZmFjdHMuanMnO1xuaW1wb3J0IHsgYnVpbGRQcm9qZWN0IH0gZnJvbSAnLi9hcGkvYnVpbGQuanMnO1xuaW1wb3J0IHsgY3JlYXRlTnBtRGlycyB9IGZyb20gJy4vYXBpL2NyZWF0ZS1ucG0tZGlycy5qcyc7XG5pbXBvcnQgeyBuZXdQcm9qZWN0IH0gZnJvbSAnLi9hcGkvbmV3LmpzJztcbmltcG9ydCB7IHByZVB1Ymxpc2ggfSBmcm9tICcuL2FwaS9wcmUtcHVibGlzaC5qcyc7XG5pbXBvcnQgeyByZW5hbWVQcm9qZWN0IH0gZnJvbSAnLi9hcGkvcmVuYW1lLmpzJztcbmltcG9ydCB7IHVuaXZlcnNhbGl6ZUJpbmFyaWVzIH0gZnJvbSAnLi9hcGkvdW5pdmVyc2FsaXplLmpzJztcbmltcG9ydCB7IHZlcnNpb24gfSBmcm9tICcuL2FwaS92ZXJzaW9uLmpzJztcbmltcG9ydCB7IEFydGlmYWN0c0NvbW1hbmQgfSBmcm9tICcuL2NvbW1hbmRzL2FydGlmYWN0cy5qcyc7XG5pbXBvcnQgeyBCdWlsZENvbW1hbmQgfSBmcm9tICcuL2NvbW1hbmRzL2J1aWxkLmpzJztcbmltcG9ydCB7IENyZWF0ZU5wbURpcnNDb21tYW5kIH0gZnJvbSAnLi9jb21tYW5kcy9jcmVhdGUtbnBtLWRpcnMuanMnO1xuaW1wb3J0IHsgTmV3Q29tbWFuZCB9IGZyb20gJy4vY29tbWFuZHMvbmV3LmpzJztcbmltcG9ydCB7IFByZVB1Ymxpc2hDb21tYW5kIH0gZnJvbSAnLi9jb21tYW5kcy9wcmUtcHVibGlzaC5qcyc7XG5pbXBvcnQgeyBSZW5hbWVDb21tYW5kIH0gZnJvbSAnLi9jb21tYW5kcy9yZW5hbWUuanMnO1xuaW1wb3J0IHsgVW5pdmVyc2FsaXplQ29tbWFuZCB9IGZyb20gJy4vY29tbWFuZHMvdW5pdmVyc2FsaXplLmpzJztcbmltcG9ydCB7IFZlcnNpb25Db21tYW5kIH0gZnJvbSAnLi9jb21tYW5kcy92ZXJzaW9uLmpzJztcbmV4cG9ydCBkZWNsYXJlIGNvbnN0IGNsaTogQ2xpPGltcG9ydChcImNsaXBhbmlvblwiKS5CYXNlQ29udGV4dD47XG4vKipcbiAqXG4gKiBAdXNhZ2VcbiAqXG4gKiBgYGB0c1xuICogY29uc3QgY2xpID0gbmV3IE5hcGlDbGkoKVxuICpcbiAqIGNsaS5idWlsZCh7XG4gKiAgIGN3ZDogJy9wYXRoL3RvL3lvdXIvcHJvamVjdCcsXG4gKiB9KVxuICogYGBgXG4gKi9cbmV4cG9ydCBkZWNsYXJlIGNsYXNzIE5hcGlDbGkge1xuICAgIGFydGlmYWN0czogdHlwZW9mIGNvbGxlY3RBcnRpZmFjdHM7XG4gICAgbmV3OiB0eXBlb2YgbmV3UHJvamVjdDtcbiAgICBidWlsZDogdHlwZW9mIGJ1aWxkUHJvamVjdDtcbiAgICBjcmVhdGVOcG1EaXJzOiB0eXBlb2YgY3JlYXRlTnBtRGlycztcbiAgICBwcmVQdWJsaXNoOiB0eXBlb2YgcHJlUHVibGlzaDtcbiAgICByZW5hbWU6IHR5cGVvZiByZW5hbWVQcm9qZWN0O1xuICAgIHVuaXZlcnNhbGl6ZTogdHlwZW9mIHVuaXZlcnNhbGl6ZUJpbmFyaWVzO1xuICAgIHZlcnNpb246IHR5cGVvZiB2ZXJzaW9uO1xufVxuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gY3JlYXRlQnVpbGRDb21tYW5kKGFyZ3M6IHN0cmluZ1tdKTogQnVpbGRDb21tYW5kO1xuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gY3JlYXRlQXJ0aWZhY3RzQ29tbWFuZChhcmdzOiBzdHJpbmdbXSk6IEFydGlmYWN0c0NvbW1hbmQ7XG5leHBvcnQgZGVjbGFyZSBmdW5jdGlvbiBjcmVhdGVDcmVhdGVOcG1EaXJzQ29tbWFuZChhcmdzOiBzdHJpbmdbXSk6IENyZWF0ZU5wbURpcnNDb21tYW5kO1xuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gY3JlYXRlUHJlUHVibGlzaENvbW1hbmQoYXJnczogc3RyaW5nW10pOiBQcmVQdWJsaXNoQ29tbWFuZDtcbmV4cG9ydCBkZWNsYXJlIGZ1bmN0aW9uIGNyZWF0ZVJlbmFtZUNvbW1hbmQoYXJnczogc3RyaW5nW10pOiBSZW5hbWVDb21tYW5kO1xuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gY3JlYXRlVW5pdmVyc2FsaXplQ29tbWFuZChhcmdzOiBzdHJpbmdbXSk6IFVuaXZlcnNhbGl6ZUNvbW1hbmQ7XG5leHBvcnQgZGVjbGFyZSBmdW5jdGlvbiBjcmVhdGVWZXJzaW9uQ29tbWFuZChhcmdzOiBzdHJpbmdbXSk6IFZlcnNpb25Db21tYW5kO1xuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gY3JlYXRlTmV3Q29tbWFuZChhcmdzOiBzdHJpbmdbXSk6IE5ld0NvbW1hbmQ7XG5leHBvcnQgeyBwYXJzZVRyaXBsZSB9IGZyb20gJy4vdXRpbHMvdGFyZ2V0LmpzJztcbmV4cG9ydCB7IHR5cGUgR2VuZXJhdGVUeXBlRGVmT3B0aW9ucywgdHlwZSBXcml0ZUpzQmluZGluZ09wdGlvbnMsIHdyaXRlSnNCaW5kaW5nLCBnZW5lcmF0ZVR5cGVEZWYsIH0gZnJvbSAnLi9hcGkvYnVpbGQuanMnO1xu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