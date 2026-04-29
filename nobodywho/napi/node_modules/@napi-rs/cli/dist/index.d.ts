import * as clipanion2 from "clipanion";
import { Cli, Command } from "clipanion";

//#region src/def/artifacts.d.ts
declare abstract class BaseArtifactsCommand extends Command {
  static paths: string[][];
  static usage: clipanion2.Usage;
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
  static usage: clipanion2.Usage;
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
  static usage: clipanion2.Usage;
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
  static usage: clipanion2.Usage;
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
  static usage: clipanion2.Usage;
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
  static usage: clipanion2.Usage;
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
  static usage: clipanion2.Usage;
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
  static usage: clipanion2.Usage;
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
  static usage: clipanion2.Usage;
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
declare const cli: Cli<clipanion2.BaseContext>;
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
//# sourceMappingURL=data:application/json;charset=utf-8;base64,eyJ2ZXJzaW9uIjozLCJmaWxlIjoiaW5kZXguZC50cyIsIm5hbWVzIjpbXSwic291cmNlcyI6WyIuLi9zcmMvZGVmL2FydGlmYWN0cy5kLnRzIiwiLi4vc3JjL2FwaS9hcnRpZmFjdHMuZC50cyIsIi4uL3NyYy9kZWYvYnVpbGQuZC50cyIsIi4uL3NyYy9hcGkvYnVpbGQuZC50cyIsIi4uL3NyYy9kZWYvY3JlYXRlLW5wbS1kaXJzLmQudHMiLCIuLi9zcmMvYXBpL2NyZWF0ZS1ucG0tZGlycy5kLnRzIiwiLi4vc3JjL2RlZi9uZXcuZC50cyIsIi4uL3NyYy9hcGkvbmV3LmQudHMiLCIuLi9zcmMvZGVmL3ByZS1wdWJsaXNoLmQudHMiLCIuLi9zcmMvYXBpL3ByZS1wdWJsaXNoLmQudHMiLCIuLi9zcmMvZGVmL3JlbmFtZS5kLnRzIiwiLi4vc3JjL2FwaS9yZW5hbWUuZC50cyIsIi4uL3NyYy9kZWYvdW5pdmVyc2FsaXplLmQudHMiLCIuLi9zcmMvYXBpL3VuaXZlcnNhbGl6ZS5kLnRzIiwiLi4vc3JjL2RlZi92ZXJzaW9uLmQudHMiLCIuLi9zcmMvYXBpL3ZlcnNpb24uZC50cyIsIi4uL3NyYy9jb21tYW5kcy9hcnRpZmFjdHMuZC50cyIsIi4uL3NyYy9jb21tYW5kcy9idWlsZC5kLnRzIiwiLi4vc3JjL2NvbW1hbmRzL2NyZWF0ZS1ucG0tZGlycy5kLnRzIiwiLi4vc3JjL2NvbW1hbmRzL25ldy5kLnRzIiwiLi4vc3JjL2NvbW1hbmRzL3ByZS1wdWJsaXNoLmQudHMiLCIuLi9zcmMvY29tbWFuZHMvcmVuYW1lLmQudHMiLCIuLi9zcmMvY29tbWFuZHMvdW5pdmVyc2FsaXplLmQudHMiLCIuLi9zcmMvY29tbWFuZHMvdmVyc2lvbi5kLnRzIiwiLi4vc3JjL3V0aWxzL3RhcmdldC5kLnRzIiwiLi4vc3JjL3V0aWxzL2NvbmZpZy5kLnRzIiwiLi4vc3JjL2luZGV4LmQudHMiXSwic291cmNlc0NvbnRlbnQiOlsiaW1wb3J0IHsgQ29tbWFuZCB9IGZyb20gJ2NsaXBhbmlvbic7XG5leHBvcnQgZGVjbGFyZSBhYnN0cmFjdCBjbGFzcyBCYXNlQXJ0aWZhY3RzQ29tbWFuZCBleHRlbmRzIENvbW1hbmQge1xuICAgIHN0YXRpYyBwYXRoczogc3RyaW5nW11bXTtcbiAgICBzdGF0aWMgdXNhZ2U6IGltcG9ydChcImNsaXBhbmlvblwiKS5Vc2FnZTtcbiAgICBjd2Q6IHN0cmluZztcbiAgICBjb25maWdQYXRoPzogc3RyaW5nO1xuICAgIHBhY2thZ2VKc29uUGF0aDogc3RyaW5nO1xuICAgIG91dHB1dERpcjogc3RyaW5nO1xuICAgIG5wbURpcjogc3RyaW5nO1xuICAgIGJ1aWxkT3V0cHV0RGlyPzogc3RyaW5nO1xuICAgIGdldE9wdGlvbnMoKToge1xuICAgICAgICBjd2Q6IHN0cmluZztcbiAgICAgICAgY29uZmlnUGF0aDogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgICAgICBwYWNrYWdlSnNvblBhdGg6IHN0cmluZztcbiAgICAgICAgb3V0cHV0RGlyOiBzdHJpbmc7XG4gICAgICAgIG5wbURpcjogc3RyaW5nO1xuICAgICAgICBidWlsZE91dHB1dERpcjogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgIH07XG59XG4vKipcbiAqIENvcHkgYXJ0aWZhY3RzIGZyb20gR2l0aHViIEFjdGlvbnMgaW50byBucG0gcGFja2FnZXMgYW5kIHJlYWR5IHRvIHB1Ymxpc2hcbiAqL1xuZXhwb3J0IGludGVyZmFjZSBBcnRpZmFjdHNPcHRpb25zIHtcbiAgICAvKipcbiAgICAgKiBUaGUgd29ya2luZyBkaXJlY3Rvcnkgb2Ygd2hlcmUgbmFwaSBjb21tYW5kIHdpbGwgYmUgZXhlY3V0ZWQgaW4sIGFsbCBvdGhlciBwYXRocyBvcHRpb25zIGFyZSByZWxhdGl2ZSB0byB0aGlzIHBhdGhcbiAgICAgKlxuICAgICAqIEBkZWZhdWx0IHByb2Nlc3MuY3dkKClcbiAgICAgKi9cbiAgICBjd2Q/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogUGF0aCB0byBgbmFwaWAgY29uZmlnIGpzb24gZmlsZVxuICAgICAqL1xuICAgIGNvbmZpZ1BhdGg/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogUGF0aCB0byBgcGFja2FnZS5qc29uYFxuICAgICAqXG4gICAgICogQGRlZmF1bHQgJ3BhY2thZ2UuanNvbidcbiAgICAgKi9cbiAgICBwYWNrYWdlSnNvblBhdGg/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogUGF0aCB0byB0aGUgZm9sZGVyIHdoZXJlIGFsbCBidWlsdCBgLm5vZGVgIGZpbGVzIHB1dCwgc2FtZSBhcyBgLS1vdXRwdXQtZGlyYCBvZiBidWlsZCBjb21tYW5kXG4gICAgICpcbiAgICAgKiBAZGVmYXVsdCAnLi9hcnRpZmFjdHMnXG4gICAgICovXG4gICAgb3V0cHV0RGlyPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gdGhlIGZvbGRlciB3aGVyZSB0aGUgbnBtIHBhY2thZ2VzIHB1dFxuICAgICAqXG4gICAgICogQGRlZmF1bHQgJ25wbSdcbiAgICAgKi9cbiAgICBucG1EaXI/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogUGF0aCB0byB0aGUgYnVpbGQgb3V0cHV0IGRpciwgb25seSBuZWVkZWQgd2hlbiB0YXJnZXRzIGNvbnRhaW5zIGB3YXNtMzItd2FzaS0qYFxuICAgICAqL1xuICAgIGJ1aWxkT3V0cHV0RGlyPzogc3RyaW5nO1xufVxuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gYXBwbHlEZWZhdWx0QXJ0aWZhY3RzT3B0aW9ucyhvcHRpb25zOiBBcnRpZmFjdHNPcHRpb25zKToge1xuICAgIGN3ZDogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gYG5hcGlgIGNvbmZpZyBqc29uIGZpbGVcbiAgICAgKi9cbiAgICBjb25maWdQYXRoPzogc3RyaW5nO1xuICAgIHBhY2thZ2VKc29uUGF0aDogc3RyaW5nO1xuICAgIG91dHB1dERpcjogc3RyaW5nO1xuICAgIG5wbURpcjogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gdGhlIGJ1aWxkIG91dHB1dCBkaXIsIG9ubHkgbmVlZGVkIHdoZW4gdGFyZ2V0cyBjb250YWlucyBgd2FzbTMyLXdhc2ktKmBcbiAgICAgKi9cbiAgICBidWlsZE91dHB1dERpcj86IHN0cmluZztcbn07XG4iLCJpbXBvcnQgeyB0eXBlIEFydGlmYWN0c09wdGlvbnMgfSBmcm9tICcuLi9kZWYvYXJ0aWZhY3RzLmpzJztcbmV4cG9ydCBkZWNsYXJlIGZ1bmN0aW9uIGNvbGxlY3RBcnRpZmFjdHModXNlck9wdGlvbnM6IEFydGlmYWN0c09wdGlvbnMpOiBQcm9taXNlPHZvaWQ+O1xuIiwiaW1wb3J0IHsgQ29tbWFuZCB9IGZyb20gJ2NsaXBhbmlvbic7XG5leHBvcnQgZGVjbGFyZSBhYnN0cmFjdCBjbGFzcyBCYXNlQnVpbGRDb21tYW5kIGV4dGVuZHMgQ29tbWFuZCB7XG4gICAgc3RhdGljIHBhdGhzOiBzdHJpbmdbXVtdO1xuICAgIHN0YXRpYyB1c2FnZTogaW1wb3J0KFwiY2xpcGFuaW9uXCIpLlVzYWdlO1xuICAgIHRhcmdldD86IHN0cmluZztcbiAgICBjd2Q/OiBzdHJpbmc7XG4gICAgbWFuaWZlc3RQYXRoPzogc3RyaW5nO1xuICAgIGNvbmZpZ1BhdGg/OiBzdHJpbmc7XG4gICAgcGFja2FnZUpzb25QYXRoPzogc3RyaW5nO1xuICAgIHRhcmdldERpcj86IHN0cmluZztcbiAgICBvdXRwdXREaXI/OiBzdHJpbmc7XG4gICAgcGxhdGZvcm0/OiBib29sZWFuO1xuICAgIGpzUGFja2FnZU5hbWU/OiBzdHJpbmc7XG4gICAgY29uc3RFbnVtPzogYm9vbGVhbjtcbiAgICBqc0JpbmRpbmc/OiBzdHJpbmc7XG4gICAgbm9Kc0JpbmRpbmc/OiBib29sZWFuO1xuICAgIGR0cz86IHN0cmluZztcbiAgICBkdHNIZWFkZXI/OiBzdHJpbmc7XG4gICAgbm9EdHNIZWFkZXI/OiBib29sZWFuO1xuICAgIGR0c0NhY2hlOiBib29sZWFuO1xuICAgIGVzbT86IGJvb2xlYW47XG4gICAgc3RyaXA/OiBib29sZWFuO1xuICAgIHJlbGVhc2U/OiBib29sZWFuO1xuICAgIHZlcmJvc2U/OiBib29sZWFuO1xuICAgIGJpbj86IHN0cmluZztcbiAgICBwYWNrYWdlPzogc3RyaW5nO1xuICAgIHByb2ZpbGU/OiBzdHJpbmc7XG4gICAgY3Jvc3NDb21waWxlPzogYm9vbGVhbjtcbiAgICB1c2VDcm9zcz86IGJvb2xlYW47XG4gICAgdXNlTmFwaUNyb3NzPzogYm9vbGVhbjtcbiAgICB3YXRjaD86IGJvb2xlYW47XG4gICAgZmVhdHVyZXM/OiBzdHJpbmdbXTtcbiAgICBhbGxGZWF0dXJlcz86IGJvb2xlYW47XG4gICAgbm9EZWZhdWx0RmVhdHVyZXM/OiBib29sZWFuO1xuICAgIGdldE9wdGlvbnMoKToge1xuICAgICAgICB0YXJnZXQ6IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICAgICAgY3dkOiBzdHJpbmcgfCB1bmRlZmluZWQ7XG4gICAgICAgIG1hbmlmZXN0UGF0aDogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgICAgICBjb25maWdQYXRoOiBzdHJpbmcgfCB1bmRlZmluZWQ7XG4gICAgICAgIHBhY2thZ2VKc29uUGF0aDogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgICAgICB0YXJnZXREaXI6IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICAgICAgb3V0cHV0RGlyOiBzdHJpbmcgfCB1bmRlZmluZWQ7XG4gICAgICAgIHBsYXRmb3JtOiBib29sZWFuIHwgdW5kZWZpbmVkO1xuICAgICAgICBqc1BhY2thZ2VOYW1lOiBzdHJpbmcgfCB1bmRlZmluZWQ7XG4gICAgICAgIGNvbnN0RW51bTogYm9vbGVhbiB8IHVuZGVmaW5lZDtcbiAgICAgICAganNCaW5kaW5nOiBzdHJpbmcgfCB1bmRlZmluZWQ7XG4gICAgICAgIG5vSnNCaW5kaW5nOiBib29sZWFuIHwgdW5kZWZpbmVkO1xuICAgICAgICBkdHM6IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICAgICAgZHRzSGVhZGVyOiBzdHJpbmcgfCB1bmRlZmluZWQ7XG4gICAgICAgIG5vRHRzSGVhZGVyOiBib29sZWFuIHwgdW5kZWZpbmVkO1xuICAgICAgICBkdHNDYWNoZTogYm9vbGVhbjtcbiAgICAgICAgZXNtOiBib29sZWFuIHwgdW5kZWZpbmVkO1xuICAgICAgICBzdHJpcDogYm9vbGVhbiB8IHVuZGVmaW5lZDtcbiAgICAgICAgcmVsZWFzZTogYm9vbGVhbiB8IHVuZGVmaW5lZDtcbiAgICAgICAgdmVyYm9zZTogYm9vbGVhbiB8IHVuZGVmaW5lZDtcbiAgICAgICAgYmluOiBzdHJpbmcgfCB1bmRlZmluZWQ7XG4gICAgICAgIHBhY2thZ2U6IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICAgICAgcHJvZmlsZTogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgICAgICBjcm9zc0NvbXBpbGU6IGJvb2xlYW4gfCB1bmRlZmluZWQ7XG4gICAgICAgIHVzZUNyb3NzOiBib29sZWFuIHwgdW5kZWZpbmVkO1xuICAgICAgICB1c2VOYXBpQ3Jvc3M6IGJvb2xlYW4gfCB1bmRlZmluZWQ7XG4gICAgICAgIHdhdGNoOiBib29sZWFuIHwgdW5kZWZpbmVkO1xuICAgICAgICBmZWF0dXJlczogc3RyaW5nW10gfCB1bmRlZmluZWQ7XG4gICAgICAgIGFsbEZlYXR1cmVzOiBib29sZWFuIHwgdW5kZWZpbmVkO1xuICAgICAgICBub0RlZmF1bHRGZWF0dXJlczogYm9vbGVhbiB8IHVuZGVmaW5lZDtcbiAgICB9O1xufVxuLyoqXG4gKiBCdWlsZCB0aGUgTkFQSS1SUyBwcm9qZWN0XG4gKi9cbmV4cG9ydCBpbnRlcmZhY2UgQnVpbGRPcHRpb25zIHtcbiAgICAvKipcbiAgICAgKiBCdWlsZCBmb3IgdGhlIHRhcmdldCB0cmlwbGUsIGJ5cGFzc2VkIHRvIGBjYXJnbyBidWlsZCAtLXRhcmdldGBcbiAgICAgKi9cbiAgICB0YXJnZXQ/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogVGhlIHdvcmtpbmcgZGlyZWN0b3J5IG9mIHdoZXJlIG5hcGkgY29tbWFuZCB3aWxsIGJlIGV4ZWN1dGVkIGluLCBhbGwgb3RoZXIgcGF0aHMgb3B0aW9ucyBhcmUgcmVsYXRpdmUgdG8gdGhpcyBwYXRoXG4gICAgICovXG4gICAgY3dkPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gYENhcmdvLnRvbWxgXG4gICAgICovXG4gICAgbWFuaWZlc3RQYXRoPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gYG5hcGlgIGNvbmZpZyBqc29uIGZpbGVcbiAgICAgKi9cbiAgICBjb25maWdQYXRoPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gYHBhY2thZ2UuanNvbmBcbiAgICAgKi9cbiAgICBwYWNrYWdlSnNvblBhdGg/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogRGlyZWN0b3J5IGZvciBhbGwgY3JhdGUgZ2VuZXJhdGVkIGFydGlmYWN0cywgc2VlIGBjYXJnbyBidWlsZCAtLXRhcmdldC1kaXJgXG4gICAgICovXG4gICAgdGFyZ2V0RGlyPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gd2hlcmUgYWxsIHRoZSBidWlsdCBmaWxlcyB3b3VsZCBiZSBwdXQuIERlZmF1bHQgdG8gdGhlIGNyYXRlIGZvbGRlclxuICAgICAqL1xuICAgIG91dHB1dERpcj86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBBZGQgcGxhdGZvcm0gdHJpcGxlIHRvIHRoZSBnZW5lcmF0ZWQgbm9kZWpzIGJpbmRpbmcgZmlsZSwgZWc6IGBbbmFtZV0ubGludXgteDY0LWdudS5ub2RlYFxuICAgICAqL1xuICAgIHBsYXRmb3JtPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBQYWNrYWdlIG5hbWUgaW4gZ2VuZXJhdGVkIGpzIGJpbmRpbmcgZmlsZS4gT25seSB3b3JrcyB3aXRoIGAtLXBsYXRmb3JtYCBmbGFnXG4gICAgICovXG4gICAganNQYWNrYWdlTmFtZT86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBXaGV0aGVyIGdlbmVyYXRlIGNvbnN0IGVudW0gZm9yIHR5cGVzY3JpcHQgYmluZGluZ3NcbiAgICAgKi9cbiAgICBjb25zdEVudW0/OiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIFBhdGggYW5kIGZpbGVuYW1lIG9mIGdlbmVyYXRlZCBKUyBiaW5kaW5nIGZpbGUuIE9ubHkgd29ya3Mgd2l0aCBgLS1wbGF0Zm9ybWAgZmxhZy4gUmVsYXRpdmUgdG8gYC0tb3V0cHV0LWRpcmAuXG4gICAgICovXG4gICAganNCaW5kaW5nPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFdoZXRoZXIgdG8gZGlzYWJsZSB0aGUgZ2VuZXJhdGlvbiBKUyBiaW5kaW5nIGZpbGUuIE9ubHkgd29ya3Mgd2l0aCBgLS1wbGF0Zm9ybWAgZmxhZy5cbiAgICAgKi9cbiAgICBub0pzQmluZGluZz86IGJvb2xlYW47XG4gICAgLyoqXG4gICAgICogUGF0aCBhbmQgZmlsZW5hbWUgb2YgZ2VuZXJhdGVkIHR5cGUgZGVmIGZpbGUuIFJlbGF0aXZlIHRvIGAtLW91dHB1dC1kaXJgXG4gICAgICovXG4gICAgZHRzPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIEN1c3RvbSBmaWxlIGhlYWRlciBmb3IgZ2VuZXJhdGVkIHR5cGUgZGVmIGZpbGUuIE9ubHkgd29ya3Mgd2hlbiBgdHlwZWRlZmAgZmVhdHVyZSBlbmFibGVkLlxuICAgICAqL1xuICAgIGR0c0hlYWRlcj86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBXaGV0aGVyIHRvIGRpc2FibGUgdGhlIGRlZmF1bHQgZmlsZSBoZWFkZXIgZm9yIGdlbmVyYXRlZCB0eXBlIGRlZiBmaWxlLiBPbmx5IHdvcmtzIHdoZW4gYHR5cGVkZWZgIGZlYXR1cmUgZW5hYmxlZC5cbiAgICAgKi9cbiAgICBub0R0c0hlYWRlcj86IGJvb2xlYW47XG4gICAgLyoqXG4gICAgICogV2hldGhlciB0byBlbmFibGUgdGhlIGR0cyBjYWNoZSwgZGVmYXVsdCB0byB0cnVlXG4gICAgICpcbiAgICAgKiBAZGVmYXVsdCB0cnVlXG4gICAgICovXG4gICAgZHRzQ2FjaGU/OiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIFdoZXRoZXIgdG8gZW1pdCBhbiBFU00gSlMgYmluZGluZyBmaWxlIGluc3RlYWQgb2YgQ0pTIGZvcm1hdC4gT25seSB3b3JrcyB3aXRoIGAtLXBsYXRmb3JtYCBmbGFnLlxuICAgICAqL1xuICAgIGVzbT86IGJvb2xlYW47XG4gICAgLyoqXG4gICAgICogV2hldGhlciBzdHJpcCB0aGUgbGlicmFyeSB0byBhY2hpZXZlIHRoZSBtaW5pbXVtIGZpbGUgc2l6ZVxuICAgICAqL1xuICAgIHN0cmlwPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBCdWlsZCBpbiByZWxlYXNlIG1vZGVcbiAgICAgKi9cbiAgICByZWxlYXNlPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBWZXJib3NlbHkgbG9nIGJ1aWxkIGNvbW1hbmQgdHJhY2VcbiAgICAgKi9cbiAgICB2ZXJib3NlPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBCdWlsZCBvbmx5IHRoZSBzcGVjaWZpZWQgYmluYXJ5XG4gICAgICovXG4gICAgYmluPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIEJ1aWxkIHRoZSBzcGVjaWZpZWQgbGlicmFyeSBvciB0aGUgb25lIGF0IGN3ZFxuICAgICAqL1xuICAgIHBhY2thZ2U/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogQnVpbGQgYXJ0aWZhY3RzIHdpdGggdGhlIHNwZWNpZmllZCBwcm9maWxlXG4gICAgICovXG4gICAgcHJvZmlsZT86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBbZXhwZXJpbWVudGFsXSBjcm9zcy1jb21waWxlIGZvciB0aGUgc3BlY2lmaWVkIHRhcmdldCB3aXRoIGBjYXJnby14d2luYCBvbiB3aW5kb3dzIGFuZCBgY2FyZ28temlnYnVpbGRgIG9uIG90aGVyIHBsYXRmb3JtXG4gICAgICovXG4gICAgY3Jvc3NDb21waWxlPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBbZXhwZXJpbWVudGFsXSB1c2UgW2Nyb3NzXShodHRwczovL2dpdGh1Yi5jb20vY3Jvc3MtcnMvY3Jvc3MpIGluc3RlYWQgb2YgYGNhcmdvYFxuICAgICAqL1xuICAgIHVzZUNyb3NzPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBbZXhwZXJpbWVudGFsXSB1c2UgQG5hcGktcnMvY3Jvc3MtdG9vbGNoYWluIHRvIGNyb3NzLWNvbXBpbGUgTGludXggYXJtL2FybTY0L3g2NCBnbnUgdGFyZ2V0cy5cbiAgICAgKi9cbiAgICB1c2VOYXBpQ3Jvc3M/OiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIHdhdGNoIHRoZSBjcmF0ZSBjaGFuZ2VzIGFuZCBidWlsZCBjb250aW51b3VzbHkgd2l0aCBgY2FyZ28td2F0Y2hgIGNyYXRlc1xuICAgICAqL1xuICAgIHdhdGNoPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBTcGFjZS1zZXBhcmF0ZWQgbGlzdCBvZiBmZWF0dXJlcyB0byBhY3RpdmF0ZVxuICAgICAqL1xuICAgIGZlYXR1cmVzPzogc3RyaW5nW107XG4gICAgLyoqXG4gICAgICogQWN0aXZhdGUgYWxsIGF2YWlsYWJsZSBmZWF0dXJlc1xuICAgICAqL1xuICAgIGFsbEZlYXR1cmVzPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBEbyBub3QgYWN0aXZhdGUgdGhlIGBkZWZhdWx0YCBmZWF0dXJlXG4gICAgICovXG4gICAgbm9EZWZhdWx0RmVhdHVyZXM/OiBib29sZWFuO1xufVxuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gYXBwbHlEZWZhdWx0QnVpbGRPcHRpb25zKG9wdGlvbnM6IEJ1aWxkT3B0aW9ucyk6IHtcbiAgICAvKipcbiAgICAgKiBCdWlsZCBmb3IgdGhlIHRhcmdldCB0cmlwbGUsIGJ5cGFzc2VkIHRvIGBjYXJnbyBidWlsZCAtLXRhcmdldGBcbiAgICAgKi9cbiAgICB0YXJnZXQ/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogVGhlIHdvcmtpbmcgZGlyZWN0b3J5IG9mIHdoZXJlIG5hcGkgY29tbWFuZCB3aWxsIGJlIGV4ZWN1dGVkIGluLCBhbGwgb3RoZXIgcGF0aHMgb3B0aW9ucyBhcmUgcmVsYXRpdmUgdG8gdGhpcyBwYXRoXG4gICAgICovXG4gICAgY3dkPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gYENhcmdvLnRvbWxgXG4gICAgICovXG4gICAgbWFuaWZlc3RQYXRoPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gYG5hcGlgIGNvbmZpZyBqc29uIGZpbGVcbiAgICAgKi9cbiAgICBjb25maWdQYXRoPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gYHBhY2thZ2UuanNvbmBcbiAgICAgKi9cbiAgICBwYWNrYWdlSnNvblBhdGg/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogRGlyZWN0b3J5IGZvciBhbGwgY3JhdGUgZ2VuZXJhdGVkIGFydGlmYWN0cywgc2VlIGBjYXJnbyBidWlsZCAtLXRhcmdldC1kaXJgXG4gICAgICovXG4gICAgdGFyZ2V0RGlyPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gd2hlcmUgYWxsIHRoZSBidWlsdCBmaWxlcyB3b3VsZCBiZSBwdXQuIERlZmF1bHQgdG8gdGhlIGNyYXRlIGZvbGRlclxuICAgICAqL1xuICAgIG91dHB1dERpcj86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBBZGQgcGxhdGZvcm0gdHJpcGxlIHRvIHRoZSBnZW5lcmF0ZWQgbm9kZWpzIGJpbmRpbmcgZmlsZSwgZWc6IGBbbmFtZV0ubGludXgteDY0LWdudS5ub2RlYFxuICAgICAqL1xuICAgIHBsYXRmb3JtPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBQYWNrYWdlIG5hbWUgaW4gZ2VuZXJhdGVkIGpzIGJpbmRpbmcgZmlsZS4gT25seSB3b3JrcyB3aXRoIGAtLXBsYXRmb3JtYCBmbGFnXG4gICAgICovXG4gICAganNQYWNrYWdlTmFtZT86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBXaGV0aGVyIGdlbmVyYXRlIGNvbnN0IGVudW0gZm9yIHR5cGVzY3JpcHQgYmluZGluZ3NcbiAgICAgKi9cbiAgICBjb25zdEVudW0/OiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIFBhdGggYW5kIGZpbGVuYW1lIG9mIGdlbmVyYXRlZCBKUyBiaW5kaW5nIGZpbGUuIE9ubHkgd29ya3Mgd2l0aCBgLS1wbGF0Zm9ybWAgZmxhZy4gUmVsYXRpdmUgdG8gYC0tb3V0cHV0LWRpcmAuXG4gICAgICovXG4gICAganNCaW5kaW5nPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFdoZXRoZXIgdG8gZGlzYWJsZSB0aGUgZ2VuZXJhdGlvbiBKUyBiaW5kaW5nIGZpbGUuIE9ubHkgd29ya3Mgd2l0aCBgLS1wbGF0Zm9ybWAgZmxhZy5cbiAgICAgKi9cbiAgICBub0pzQmluZGluZz86IGJvb2xlYW47XG4gICAgLyoqXG4gICAgICogUGF0aCBhbmQgZmlsZW5hbWUgb2YgZ2VuZXJhdGVkIHR5cGUgZGVmIGZpbGUuIFJlbGF0aXZlIHRvIGAtLW91dHB1dC1kaXJgXG4gICAgICovXG4gICAgZHRzPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIEN1c3RvbSBmaWxlIGhlYWRlciBmb3IgZ2VuZXJhdGVkIHR5cGUgZGVmIGZpbGUuIE9ubHkgd29ya3Mgd2hlbiBgdHlwZWRlZmAgZmVhdHVyZSBlbmFibGVkLlxuICAgICAqL1xuICAgIGR0c0hlYWRlcj86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBXaGV0aGVyIHRvIGRpc2FibGUgdGhlIGRlZmF1bHQgZmlsZSBoZWFkZXIgZm9yIGdlbmVyYXRlZCB0eXBlIGRlZiBmaWxlLiBPbmx5IHdvcmtzIHdoZW4gYHR5cGVkZWZgIGZlYXR1cmUgZW5hYmxlZC5cbiAgICAgKi9cbiAgICBub0R0c0hlYWRlcj86IGJvb2xlYW47XG4gICAgZHRzQ2FjaGU6IGJvb2xlYW47XG4gICAgLyoqXG4gICAgICogV2hldGhlciB0byBlbWl0IGFuIEVTTSBKUyBiaW5kaW5nIGZpbGUgaW5zdGVhZCBvZiBDSlMgZm9ybWF0LiBPbmx5IHdvcmtzIHdpdGggYC0tcGxhdGZvcm1gIGZsYWcuXG4gICAgICovXG4gICAgZXNtPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBXaGV0aGVyIHN0cmlwIHRoZSBsaWJyYXJ5IHRvIGFjaGlldmUgdGhlIG1pbmltdW0gZmlsZSBzaXplXG4gICAgICovXG4gICAgc3RyaXA/OiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIEJ1aWxkIGluIHJlbGVhc2UgbW9kZVxuICAgICAqL1xuICAgIHJlbGVhc2U/OiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIFZlcmJvc2VseSBsb2cgYnVpbGQgY29tbWFuZCB0cmFjZVxuICAgICAqL1xuICAgIHZlcmJvc2U/OiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIEJ1aWxkIG9ubHkgdGhlIHNwZWNpZmllZCBiaW5hcnlcbiAgICAgKi9cbiAgICBiaW4/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogQnVpbGQgdGhlIHNwZWNpZmllZCBsaWJyYXJ5IG9yIHRoZSBvbmUgYXQgY3dkXG4gICAgICovXG4gICAgcGFja2FnZT86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBCdWlsZCBhcnRpZmFjdHMgd2l0aCB0aGUgc3BlY2lmaWVkIHByb2ZpbGVcbiAgICAgKi9cbiAgICBwcm9maWxlPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFtleHBlcmltZW50YWxdIGNyb3NzLWNvbXBpbGUgZm9yIHRoZSBzcGVjaWZpZWQgdGFyZ2V0IHdpdGggYGNhcmdvLXh3aW5gIG9uIHdpbmRvd3MgYW5kIGBjYXJnby16aWdidWlsZGAgb24gb3RoZXIgcGxhdGZvcm1cbiAgICAgKi9cbiAgICBjcm9zc0NvbXBpbGU/OiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIFtleHBlcmltZW50YWxdIHVzZSBbY3Jvc3NdKGh0dHBzOi8vZ2l0aHViLmNvbS9jcm9zcy1ycy9jcm9zcykgaW5zdGVhZCBvZiBgY2FyZ29gXG4gICAgICovXG4gICAgdXNlQ3Jvc3M/OiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIFtleHBlcmltZW50YWxdIHVzZSBAbmFwaS1ycy9jcm9zcy10b29sY2hhaW4gdG8gY3Jvc3MtY29tcGlsZSBMaW51eCBhcm0vYXJtNjQveDY0IGdudSB0YXJnZXRzLlxuICAgICAqL1xuICAgIHVzZU5hcGlDcm9zcz86IGJvb2xlYW47XG4gICAgLyoqXG4gICAgICogd2F0Y2ggdGhlIGNyYXRlIGNoYW5nZXMgYW5kIGJ1aWxkIGNvbnRpbnVvdXNseSB3aXRoIGBjYXJnby13YXRjaGAgY3JhdGVzXG4gICAgICovXG4gICAgd2F0Y2g/OiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIFNwYWNlLXNlcGFyYXRlZCBsaXN0IG9mIGZlYXR1cmVzIHRvIGFjdGl2YXRlXG4gICAgICovXG4gICAgZmVhdHVyZXM/OiBzdHJpbmdbXTtcbiAgICAvKipcbiAgICAgKiBBY3RpdmF0ZSBhbGwgYXZhaWxhYmxlIGZlYXR1cmVzXG4gICAgICovXG4gICAgYWxsRmVhdHVyZXM/OiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIERvIG5vdCBhY3RpdmF0ZSB0aGUgYGRlZmF1bHRgIGZlYXR1cmVcbiAgICAgKi9cbiAgICBub0RlZmF1bHRGZWF0dXJlcz86IGJvb2xlYW47XG59O1xuIiwiaW1wb3J0IHR5cGUgeyBCdWlsZE9wdGlvbnMgYXMgUmF3QnVpbGRPcHRpb25zIH0gZnJvbSAnLi4vZGVmL2J1aWxkLmpzJztcbnR5cGUgT3V0cHV0S2luZCA9ICdqcycgfCAnZHRzJyB8ICdub2RlJyB8ICdleGUnIHwgJ3dhc20nO1xudHlwZSBPdXRwdXQgPSB7XG4gICAga2luZDogT3V0cHV0S2luZDtcbiAgICBwYXRoOiBzdHJpbmc7XG59O1xudHlwZSBCdWlsZE9wdGlvbnMgPSBSYXdCdWlsZE9wdGlvbnMgJiB7XG4gICAgY2FyZ29PcHRpb25zPzogc3RyaW5nW107XG59O1xuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gYnVpbGRQcm9qZWN0KHJhd09wdGlvbnM6IEJ1aWxkT3B0aW9ucyk6IFByb21pc2U8e1xuICAgIHRhc2s6IFByb21pc2U8T3V0cHV0W10+O1xuICAgIGFib3J0OiAoKSA9PiB2b2lkO1xufT47XG5leHBvcnQgaW50ZXJmYWNlIFdyaXRlSnNCaW5kaW5nT3B0aW9ucyB7XG4gICAgcGxhdGZvcm0/OiBib29sZWFuO1xuICAgIG5vSnNCaW5kaW5nPzogYm9vbGVhbjtcbiAgICBpZGVudHM6IHN0cmluZ1tdO1xuICAgIGpzQmluZGluZz86IHN0cmluZztcbiAgICBlc20/OiBib29sZWFuO1xuICAgIGJpbmFyeU5hbWU6IHN0cmluZztcbiAgICBwYWNrYWdlTmFtZTogc3RyaW5nO1xuICAgIHZlcnNpb246IHN0cmluZztcbiAgICBvdXRwdXREaXI6IHN0cmluZztcbn1cbmV4cG9ydCBkZWNsYXJlIGZ1bmN0aW9uIHdyaXRlSnNCaW5kaW5nKG9wdGlvbnM6IFdyaXRlSnNCaW5kaW5nT3B0aW9ucyk6IFByb21pc2U8T3V0cHV0IHwgdW5kZWZpbmVkPjtcbmV4cG9ydCBpbnRlcmZhY2UgR2VuZXJhdGVUeXBlRGVmT3B0aW9ucyB7XG4gICAgdHlwZURlZkRpcjogc3RyaW5nO1xuICAgIG5vRHRzSGVhZGVyPzogYm9vbGVhbjtcbiAgICBkdHNIZWFkZXI/OiBzdHJpbmc7XG4gICAgZHRzSGVhZGVyRmlsZT86IHN0cmluZztcbiAgICBjb25maWdEdHNIZWFkZXI/OiBzdHJpbmc7XG4gICAgY29uZmlnRHRzSGVhZGVyRmlsZT86IHN0cmluZztcbiAgICBjb25zdEVudW0/OiBib29sZWFuO1xuICAgIGN3ZDogc3RyaW5nO1xufVxuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gZ2VuZXJhdGVUeXBlRGVmKG9wdGlvbnM6IEdlbmVyYXRlVHlwZURlZk9wdGlvbnMpOiBQcm9taXNlPHtcbiAgICBleHBvcnRzOiBzdHJpbmdbXTtcbiAgICBkdHM6IHN0cmluZztcbn0+O1xuZXhwb3J0IHt9O1xuIiwiaW1wb3J0IHsgQ29tbWFuZCB9IGZyb20gJ2NsaXBhbmlvbic7XG5leHBvcnQgZGVjbGFyZSBhYnN0cmFjdCBjbGFzcyBCYXNlQ3JlYXRlTnBtRGlyc0NvbW1hbmQgZXh0ZW5kcyBDb21tYW5kIHtcbiAgICBzdGF0aWMgcGF0aHM6IHN0cmluZ1tdW107XG4gICAgc3RhdGljIHVzYWdlOiBpbXBvcnQoXCJjbGlwYW5pb25cIikuVXNhZ2U7XG4gICAgY3dkOiBzdHJpbmc7XG4gICAgY29uZmlnUGF0aD86IHN0cmluZztcbiAgICBwYWNrYWdlSnNvblBhdGg6IHN0cmluZztcbiAgICBucG1EaXI6IHN0cmluZztcbiAgICBkcnlSdW46IGJvb2xlYW47XG4gICAgZ2V0T3B0aW9ucygpOiB7XG4gICAgICAgIGN3ZDogc3RyaW5nO1xuICAgICAgICBjb25maWdQYXRoOiBzdHJpbmcgfCB1bmRlZmluZWQ7XG4gICAgICAgIHBhY2thZ2VKc29uUGF0aDogc3RyaW5nO1xuICAgICAgICBucG1EaXI6IHN0cmluZztcbiAgICAgICAgZHJ5UnVuOiBib29sZWFuO1xuICAgIH07XG59XG4vKipcbiAqIENyZWF0ZSBucG0gcGFja2FnZSBkaXJzIGZvciBkaWZmZXJlbnQgcGxhdGZvcm1zXG4gKi9cbmV4cG9ydCBpbnRlcmZhY2UgQ3JlYXRlTnBtRGlyc09wdGlvbnMge1xuICAgIC8qKlxuICAgICAqIFRoZSB3b3JraW5nIGRpcmVjdG9yeSBvZiB3aGVyZSBuYXBpIGNvbW1hbmQgd2lsbCBiZSBleGVjdXRlZCBpbiwgYWxsIG90aGVyIHBhdGhzIG9wdGlvbnMgYXJlIHJlbGF0aXZlIHRvIHRoaXMgcGF0aFxuICAgICAqXG4gICAgICogQGRlZmF1bHQgcHJvY2Vzcy5jd2QoKVxuICAgICAqL1xuICAgIGN3ZD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIGBuYXBpYCBjb25maWcganNvbiBmaWxlXG4gICAgICovXG4gICAgY29uZmlnUGF0aD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIGBwYWNrYWdlLmpzb25gXG4gICAgICpcbiAgICAgKiBAZGVmYXVsdCAncGFja2FnZS5qc29uJ1xuICAgICAqL1xuICAgIHBhY2thZ2VKc29uUGF0aD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIHRoZSBmb2xkZXIgd2hlcmUgdGhlIG5wbSBwYWNrYWdlcyBwdXRcbiAgICAgKlxuICAgICAqIEBkZWZhdWx0ICducG0nXG4gICAgICovXG4gICAgbnBtRGlyPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIERyeSBydW4gd2l0aG91dCB0b3VjaGluZyBmaWxlIHN5c3RlbVxuICAgICAqXG4gICAgICogQGRlZmF1bHQgZmFsc2VcbiAgICAgKi9cbiAgICBkcnlSdW4/OiBib29sZWFuO1xufVxuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gYXBwbHlEZWZhdWx0Q3JlYXRlTnBtRGlyc09wdGlvbnMob3B0aW9uczogQ3JlYXRlTnBtRGlyc09wdGlvbnMpOiB7XG4gICAgY3dkOiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogUGF0aCB0byBgbmFwaWAgY29uZmlnIGpzb24gZmlsZVxuICAgICAqL1xuICAgIGNvbmZpZ1BhdGg/OiBzdHJpbmc7XG4gICAgcGFja2FnZUpzb25QYXRoOiBzdHJpbmc7XG4gICAgbnBtRGlyOiBzdHJpbmc7XG4gICAgZHJ5UnVuOiBib29sZWFuO1xufTtcbiIsImltcG9ydCB7IHR5cGUgQ3JlYXRlTnBtRGlyc09wdGlvbnMgfSBmcm9tICcuLi9kZWYvY3JlYXRlLW5wbS1kaXJzLmpzJztcbmV4cG9ydCBpbnRlcmZhY2UgUGFja2FnZU1ldGEge1xuICAgICdkaXN0LXRhZ3MnOiB7XG4gICAgICAgIFtpbmRleDogc3RyaW5nXTogc3RyaW5nO1xuICAgIH07XG59XG5leHBvcnQgZGVjbGFyZSBmdW5jdGlvbiBjcmVhdGVOcG1EaXJzKHVzZXJPcHRpb25zOiBDcmVhdGVOcG1EaXJzT3B0aW9ucyk6IFByb21pc2U8dm9pZD47XG4iLCJpbXBvcnQgeyBDb21tYW5kIH0gZnJvbSAnY2xpcGFuaW9uJztcbmV4cG9ydCBkZWNsYXJlIGFic3RyYWN0IGNsYXNzIEJhc2VOZXdDb21tYW5kIGV4dGVuZHMgQ29tbWFuZCB7XG4gICAgc3RhdGljIHBhdGhzOiBzdHJpbmdbXVtdO1xuICAgIHN0YXRpYyB1c2FnZTogaW1wb3J0KFwiY2xpcGFuaW9uXCIpLlVzYWdlO1xuICAgICQkcGF0aDogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgICQkbmFtZT86IHN0cmluZztcbiAgICBtaW5Ob2RlQXBpVmVyc2lvbjogbnVtYmVyO1xuICAgIHBhY2thZ2VNYW5hZ2VyOiBzdHJpbmc7XG4gICAgbGljZW5zZTogc3RyaW5nO1xuICAgIHRhcmdldHM6IHN0cmluZ1tdO1xuICAgIGVuYWJsZURlZmF1bHRUYXJnZXRzOiBib29sZWFuO1xuICAgIGVuYWJsZUFsbFRhcmdldHM6IGJvb2xlYW47XG4gICAgZW5hYmxlVHlwZURlZjogYm9vbGVhbjtcbiAgICBlbmFibGVHaXRodWJBY3Rpb25zOiBib29sZWFuO1xuICAgIHRlc3RGcmFtZXdvcms6IHN0cmluZztcbiAgICBkcnlSdW46IGJvb2xlYW47XG4gICAgZ2V0T3B0aW9ucygpOiB7XG4gICAgICAgIHBhdGg6IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICAgICAgbmFtZTogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgICAgICBtaW5Ob2RlQXBpVmVyc2lvbjogbnVtYmVyO1xuICAgICAgICBwYWNrYWdlTWFuYWdlcjogc3RyaW5nO1xuICAgICAgICBsaWNlbnNlOiBzdHJpbmc7XG4gICAgICAgIHRhcmdldHM6IHN0cmluZ1tdO1xuICAgICAgICBlbmFibGVEZWZhdWx0VGFyZ2V0czogYm9vbGVhbjtcbiAgICAgICAgZW5hYmxlQWxsVGFyZ2V0czogYm9vbGVhbjtcbiAgICAgICAgZW5hYmxlVHlwZURlZjogYm9vbGVhbjtcbiAgICAgICAgZW5hYmxlR2l0aHViQWN0aW9uczogYm9vbGVhbjtcbiAgICAgICAgdGVzdEZyYW1ld29yazogc3RyaW5nO1xuICAgICAgICBkcnlSdW46IGJvb2xlYW47XG4gICAgfTtcbn1cbi8qKlxuICogQ3JlYXRlIGEgbmV3IHByb2plY3Qgd2l0aCBwcmUtY29uZmlndXJlZCBib2lsZXJwbGF0ZVxuICovXG5leHBvcnQgaW50ZXJmYWNlIE5ld09wdGlvbnMge1xuICAgIC8qKlxuICAgICAqIFRoZSBwYXRoIHdoZXJlIHRoZSBOQVBJLVJTIHByb2plY3Qgd2lsbCBiZSBjcmVhdGVkLlxuICAgICAqL1xuICAgIHBhdGg/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogVGhlIG5hbWUgb2YgdGhlIHByb2plY3QsIGRlZmF1bHQgdG8gdGhlIG5hbWUgb2YgdGhlIGRpcmVjdG9yeSBpZiBub3QgcHJvdmlkZWRcbiAgICAgKi9cbiAgICBuYW1lPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFRoZSBtaW5pbXVtIE5vZGUtQVBJIHZlcnNpb24gdG8gc3VwcG9ydFxuICAgICAqXG4gICAgICogQGRlZmF1bHQgNFxuICAgICAqL1xuICAgIG1pbk5vZGVBcGlWZXJzaW9uPzogbnVtYmVyO1xuICAgIC8qKlxuICAgICAqIFRoZSBwYWNrYWdlIG1hbmFnZXIgdG8gdXNlLiBPbmx5IHN1cHBvcnQgeWFybiA0LnggZm9yIG5vdy5cbiAgICAgKlxuICAgICAqIEBkZWZhdWx0ICd5YXJuJ1xuICAgICAqL1xuICAgIHBhY2thZ2VNYW5hZ2VyPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIExpY2Vuc2UgZm9yIG9wZW4tc291cmNlZCBwcm9qZWN0XG4gICAgICpcbiAgICAgKiBAZGVmYXVsdCAnTUlUJ1xuICAgICAqL1xuICAgIGxpY2Vuc2U/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogQWxsIHRhcmdldHMgdGhlIGNyYXRlIHdpbGwgYmUgY29tcGlsZWQgZm9yLlxuICAgICAqXG4gICAgICogQGRlZmF1bHQgW11cbiAgICAgKi9cbiAgICB0YXJnZXRzPzogc3RyaW5nW107XG4gICAgLyoqXG4gICAgICogV2hldGhlciBlbmFibGUgZGVmYXVsdCB0YXJnZXRzXG4gICAgICpcbiAgICAgKiBAZGVmYXVsdCB0cnVlXG4gICAgICovXG4gICAgZW5hYmxlRGVmYXVsdFRhcmdldHM/OiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIFdoZXRoZXIgZW5hYmxlIGFsbCB0YXJnZXRzXG4gICAgICpcbiAgICAgKiBAZGVmYXVsdCBmYWxzZVxuICAgICAqL1xuICAgIGVuYWJsZUFsbFRhcmdldHM/OiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIFdoZXRoZXIgZW5hYmxlIHRoZSBgdHlwZS1kZWZgIGZlYXR1cmUgZm9yIHR5cGVzY3JpcHQgZGVmaW5pdGlvbnMgYXV0by1nZW5lcmF0aW9uXG4gICAgICpcbiAgICAgKiBAZGVmYXVsdCB0cnVlXG4gICAgICovXG4gICAgZW5hYmxlVHlwZURlZj86IGJvb2xlYW47XG4gICAgLyoqXG4gICAgICogV2hldGhlciBnZW5lcmF0ZSBwcmVjb25maWd1cmVkIEdpdEh1YiBBY3Rpb25zIHdvcmtmbG93XG4gICAgICpcbiAgICAgKiBAZGVmYXVsdCB0cnVlXG4gICAgICovXG4gICAgZW5hYmxlR2l0aHViQWN0aW9ucz86IGJvb2xlYW47XG4gICAgLyoqXG4gICAgICogVGhlIEphdmFTY3JpcHQgdGVzdCBmcmFtZXdvcmsgdG8gdXNlLCBvbmx5IHN1cHBvcnQgYGF2YWAgZm9yIG5vd1xuICAgICAqXG4gICAgICogQGRlZmF1bHQgJ2F2YSdcbiAgICAgKi9cbiAgICB0ZXN0RnJhbWV3b3JrPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFdoZXRoZXIgdG8gcnVuIHRoZSBjb21tYW5kIGluIGRyeS1ydW4gbW9kZVxuICAgICAqXG4gICAgICogQGRlZmF1bHQgZmFsc2VcbiAgICAgKi9cbiAgICBkcnlSdW4/OiBib29sZWFuO1xufVxuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gYXBwbHlEZWZhdWx0TmV3T3B0aW9ucyhvcHRpb25zOiBOZXdPcHRpb25zKToge1xuICAgIC8qKlxuICAgICAqIFRoZSBwYXRoIHdoZXJlIHRoZSBOQVBJLVJTIHByb2plY3Qgd2lsbCBiZSBjcmVhdGVkLlxuICAgICAqL1xuICAgIHBhdGg/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogVGhlIG5hbWUgb2YgdGhlIHByb2plY3QsIGRlZmF1bHQgdG8gdGhlIG5hbWUgb2YgdGhlIGRpcmVjdG9yeSBpZiBub3QgcHJvdmlkZWRcbiAgICAgKi9cbiAgICBuYW1lPzogc3RyaW5nO1xuICAgIG1pbk5vZGVBcGlWZXJzaW9uOiBudW1iZXI7XG4gICAgcGFja2FnZU1hbmFnZXI6IHN0cmluZztcbiAgICBsaWNlbnNlOiBzdHJpbmc7XG4gICAgdGFyZ2V0czogc3RyaW5nW107XG4gICAgZW5hYmxlRGVmYXVsdFRhcmdldHM6IGJvb2xlYW47XG4gICAgZW5hYmxlQWxsVGFyZ2V0czogYm9vbGVhbjtcbiAgICBlbmFibGVUeXBlRGVmOiBib29sZWFuO1xuICAgIGVuYWJsZUdpdGh1YkFjdGlvbnM6IGJvb2xlYW47XG4gICAgdGVzdEZyYW1ld29yazogc3RyaW5nO1xuICAgIGRyeVJ1bjogYm9vbGVhbjtcbn07XG4iLCJpbXBvcnQgeyB0eXBlIE5ld09wdGlvbnMgYXMgUmF3TmV3T3B0aW9ucyB9IGZyb20gJy4uL2RlZi9uZXcuanMnO1xudHlwZSBOZXdPcHRpb25zID0gUmVxdWlyZWQ8UmF3TmV3T3B0aW9ucz47XG5leHBvcnQgZGVjbGFyZSBmdW5jdGlvbiBuZXdQcm9qZWN0KHVzZXJPcHRpb25zOiBSYXdOZXdPcHRpb25zKTogUHJvbWlzZTx2b2lkPjtcbmV4cG9ydCB0eXBlIHsgTmV3T3B0aW9ucyB9O1xuIiwiaW1wb3J0IHsgQ29tbWFuZCB9IGZyb20gJ2NsaXBhbmlvbic7XG5leHBvcnQgZGVjbGFyZSBhYnN0cmFjdCBjbGFzcyBCYXNlUHJlUHVibGlzaENvbW1hbmQgZXh0ZW5kcyBDb21tYW5kIHtcbiAgICBzdGF0aWMgcGF0aHM6IHN0cmluZ1tdW107XG4gICAgc3RhdGljIHVzYWdlOiBpbXBvcnQoXCJjbGlwYW5pb25cIikuVXNhZ2U7XG4gICAgY3dkOiBzdHJpbmc7XG4gICAgY29uZmlnUGF0aD86IHN0cmluZztcbiAgICBwYWNrYWdlSnNvblBhdGg6IHN0cmluZztcbiAgICBucG1EaXI6IHN0cmluZztcbiAgICB0YWdTdHlsZTogc3RyaW5nO1xuICAgIGdoUmVsZWFzZTogYm9vbGVhbjtcbiAgICBnaFJlbGVhc2VOYW1lPzogc3RyaW5nO1xuICAgIGdoUmVsZWFzZUlkPzogc3RyaW5nO1xuICAgIHNraXBPcHRpb25hbFB1Ymxpc2g6IGJvb2xlYW47XG4gICAgZHJ5UnVuOiBib29sZWFuO1xuICAgIGdldE9wdGlvbnMoKToge1xuICAgICAgICBjd2Q6IHN0cmluZztcbiAgICAgICAgY29uZmlnUGF0aDogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgICAgICBwYWNrYWdlSnNvblBhdGg6IHN0cmluZztcbiAgICAgICAgbnBtRGlyOiBzdHJpbmc7XG4gICAgICAgIHRhZ1N0eWxlOiBzdHJpbmc7XG4gICAgICAgIGdoUmVsZWFzZTogYm9vbGVhbjtcbiAgICAgICAgZ2hSZWxlYXNlTmFtZTogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgICAgICBnaFJlbGVhc2VJZDogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgICAgICBza2lwT3B0aW9uYWxQdWJsaXNoOiBib29sZWFuO1xuICAgICAgICBkcnlSdW46IGJvb2xlYW47XG4gICAgfTtcbn1cbi8qKlxuICogVXBkYXRlIHBhY2thZ2UuanNvbiBhbmQgY29weSBhZGRvbnMgaW50byBwZXIgcGxhdGZvcm0gcGFja2FnZXNcbiAqL1xuZXhwb3J0IGludGVyZmFjZSBQcmVQdWJsaXNoT3B0aW9ucyB7XG4gICAgLyoqXG4gICAgICogVGhlIHdvcmtpbmcgZGlyZWN0b3J5IG9mIHdoZXJlIG5hcGkgY29tbWFuZCB3aWxsIGJlIGV4ZWN1dGVkIGluLCBhbGwgb3RoZXIgcGF0aHMgb3B0aW9ucyBhcmUgcmVsYXRpdmUgdG8gdGhpcyBwYXRoXG4gICAgICpcbiAgICAgKiBAZGVmYXVsdCBwcm9jZXNzLmN3ZCgpXG4gICAgICovXG4gICAgY3dkPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gYG5hcGlgIGNvbmZpZyBqc29uIGZpbGVcbiAgICAgKi9cbiAgICBjb25maWdQYXRoPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gYHBhY2thZ2UuanNvbmBcbiAgICAgKlxuICAgICAqIEBkZWZhdWx0ICdwYWNrYWdlLmpzb24nXG4gICAgICovXG4gICAgcGFja2FnZUpzb25QYXRoPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gdGhlIGZvbGRlciB3aGVyZSB0aGUgbnBtIHBhY2thZ2VzIHB1dFxuICAgICAqXG4gICAgICogQGRlZmF1bHQgJ25wbSdcbiAgICAgKi9cbiAgICBucG1EaXI/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogZ2l0IHRhZyBzdHlsZSwgYG5wbWAgb3IgYGxlcm5hYFxuICAgICAqXG4gICAgICogQGRlZmF1bHQgJ2xlcm5hJ1xuICAgICAqL1xuICAgIHRhZ1N0eWxlPzogJ25wbScgfCAnbGVybmEnO1xuICAgIC8qKlxuICAgICAqIFdoZXRoZXIgY3JlYXRlIEdpdEh1YiByZWxlYXNlXG4gICAgICpcbiAgICAgKiBAZGVmYXVsdCB0cnVlXG4gICAgICovXG4gICAgZ2hSZWxlYXNlPzogYm9vbGVhbjtcbiAgICAvKipcbiAgICAgKiBHaXRIdWIgcmVsZWFzZSBuYW1lXG4gICAgICovXG4gICAgZ2hSZWxlYXNlTmFtZT86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBFeGlzdGluZyBHaXRIdWIgcmVsZWFzZSBpZFxuICAgICAqL1xuICAgIGdoUmVsZWFzZUlkPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFdoZXRoZXIgc2tpcCBvcHRpb25hbERlcGVuZGVuY2llcyBwYWNrYWdlcyBwdWJsaXNoXG4gICAgICpcbiAgICAgKiBAZGVmYXVsdCBmYWxzZVxuICAgICAqL1xuICAgIHNraXBPcHRpb25hbFB1Ymxpc2g/OiBib29sZWFuO1xuICAgIC8qKlxuICAgICAqIERyeSBydW4gd2l0aG91dCB0b3VjaGluZyBmaWxlIHN5c3RlbVxuICAgICAqXG4gICAgICogQGRlZmF1bHQgZmFsc2VcbiAgICAgKi9cbiAgICBkcnlSdW4/OiBib29sZWFuO1xufVxuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gYXBwbHlEZWZhdWx0UHJlUHVibGlzaE9wdGlvbnMob3B0aW9uczogUHJlUHVibGlzaE9wdGlvbnMpOiB7XG4gICAgY3dkOiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogUGF0aCB0byBgbmFwaWAgY29uZmlnIGpzb24gZmlsZVxuICAgICAqL1xuICAgIGNvbmZpZ1BhdGg/OiBzdHJpbmc7XG4gICAgcGFja2FnZUpzb25QYXRoOiBzdHJpbmc7XG4gICAgbnBtRGlyOiBzdHJpbmc7XG4gICAgdGFnU3R5bGU6IHN0cmluZztcbiAgICBnaFJlbGVhc2U6IGJvb2xlYW47XG4gICAgLyoqXG4gICAgICogR2l0SHViIHJlbGVhc2UgbmFtZVxuICAgICAqL1xuICAgIGdoUmVsZWFzZU5hbWU/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogRXhpc3RpbmcgR2l0SHViIHJlbGVhc2UgaWRcbiAgICAgKi9cbiAgICBnaFJlbGVhc2VJZD86IHN0cmluZztcbiAgICBza2lwT3B0aW9uYWxQdWJsaXNoOiBib29sZWFuO1xuICAgIGRyeVJ1bjogYm9vbGVhbjtcbn07XG4iLCJpbXBvcnQgeyB0eXBlIFByZVB1Ymxpc2hPcHRpb25zIH0gZnJvbSAnLi4vZGVmL3ByZS1wdWJsaXNoLmpzJztcbmV4cG9ydCBkZWNsYXJlIGZ1bmN0aW9uIHByZVB1Ymxpc2godXNlck9wdGlvbnM6IFByZVB1Ymxpc2hPcHRpb25zKTogUHJvbWlzZTx2b2lkPjtcbiIsImltcG9ydCB7IENvbW1hbmQgfSBmcm9tICdjbGlwYW5pb24nO1xuZXhwb3J0IGRlY2xhcmUgYWJzdHJhY3QgY2xhc3MgQmFzZVJlbmFtZUNvbW1hbmQgZXh0ZW5kcyBDb21tYW5kIHtcbiAgICBzdGF0aWMgcGF0aHM6IHN0cmluZ1tdW107XG4gICAgc3RhdGljIHVzYWdlOiBpbXBvcnQoXCJjbGlwYW5pb25cIikuVXNhZ2U7XG4gICAgY3dkOiBzdHJpbmc7XG4gICAgY29uZmlnUGF0aD86IHN0cmluZztcbiAgICBwYWNrYWdlSnNvblBhdGg6IHN0cmluZztcbiAgICBucG1EaXI6IHN0cmluZztcbiAgICAkJG5hbWU/OiBzdHJpbmc7XG4gICAgYmluYXJ5TmFtZT86IHN0cmluZztcbiAgICBwYWNrYWdlTmFtZT86IHN0cmluZztcbiAgICBtYW5pZmVzdFBhdGg6IHN0cmluZztcbiAgICByZXBvc2l0b3J5Pzogc3RyaW5nO1xuICAgIGRlc2NyaXB0aW9uPzogc3RyaW5nO1xuICAgIGdldE9wdGlvbnMoKToge1xuICAgICAgICBjd2Q6IHN0cmluZztcbiAgICAgICAgY29uZmlnUGF0aDogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgICAgICBwYWNrYWdlSnNvblBhdGg6IHN0cmluZztcbiAgICAgICAgbnBtRGlyOiBzdHJpbmc7XG4gICAgICAgIG5hbWU6IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICAgICAgYmluYXJ5TmFtZTogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgICAgICBwYWNrYWdlTmFtZTogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgICAgICBtYW5pZmVzdFBhdGg6IHN0cmluZztcbiAgICAgICAgcmVwb3NpdG9yeTogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgICAgICBkZXNjcmlwdGlvbjogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgIH07XG59XG4vKipcbiAqIFJlbmFtZSB0aGUgTkFQSS1SUyBwcm9qZWN0XG4gKi9cbmV4cG9ydCBpbnRlcmZhY2UgUmVuYW1lT3B0aW9ucyB7XG4gICAgLyoqXG4gICAgICogVGhlIHdvcmtpbmcgZGlyZWN0b3J5IG9mIHdoZXJlIG5hcGkgY29tbWFuZCB3aWxsIGJlIGV4ZWN1dGVkIGluLCBhbGwgb3RoZXIgcGF0aHMgb3B0aW9ucyBhcmUgcmVsYXRpdmUgdG8gdGhpcyBwYXRoXG4gICAgICpcbiAgICAgKiBAZGVmYXVsdCBwcm9jZXNzLmN3ZCgpXG4gICAgICovXG4gICAgY3dkPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gYG5hcGlgIGNvbmZpZyBqc29uIGZpbGVcbiAgICAgKi9cbiAgICBjb25maWdQYXRoPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gYHBhY2thZ2UuanNvbmBcbiAgICAgKlxuICAgICAqIEBkZWZhdWx0ICdwYWNrYWdlLmpzb24nXG4gICAgICovXG4gICAgcGFja2FnZUpzb25QYXRoPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gdGhlIGZvbGRlciB3aGVyZSB0aGUgbnBtIHBhY2thZ2VzIHB1dFxuICAgICAqXG4gICAgICogQGRlZmF1bHQgJ25wbSdcbiAgICAgKi9cbiAgICBucG1EaXI/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogVGhlIG5ldyBuYW1lIG9mIHRoZSBwcm9qZWN0XG4gICAgICovXG4gICAgbmFtZT86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBUaGUgbmV3IGJpbmFyeSBuYW1lICoubm9kZSBmaWxlc1xuICAgICAqL1xuICAgIGJpbmFyeU5hbWU/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogVGhlIG5ldyBwYWNrYWdlIG5hbWUgb2YgdGhlIHByb2plY3RcbiAgICAgKi9cbiAgICBwYWNrYWdlTmFtZT86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIGBDYXJnby50b21sYFxuICAgICAqXG4gICAgICogQGRlZmF1bHQgJ0NhcmdvLnRvbWwnXG4gICAgICovXG4gICAgbWFuaWZlc3RQYXRoPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFRoZSBuZXcgcmVwb3NpdG9yeSBvZiB0aGUgcHJvamVjdFxuICAgICAqL1xuICAgIHJlcG9zaXRvcnk/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogVGhlIG5ldyBkZXNjcmlwdGlvbiBvZiB0aGUgcHJvamVjdFxuICAgICAqL1xuICAgIGRlc2NyaXB0aW9uPzogc3RyaW5nO1xufVxuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gYXBwbHlEZWZhdWx0UmVuYW1lT3B0aW9ucyhvcHRpb25zOiBSZW5hbWVPcHRpb25zKToge1xuICAgIGN3ZDogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gYG5hcGlgIGNvbmZpZyBqc29uIGZpbGVcbiAgICAgKi9cbiAgICBjb25maWdQYXRoPzogc3RyaW5nO1xuICAgIHBhY2thZ2VKc29uUGF0aDogc3RyaW5nO1xuICAgIG5wbURpcjogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFRoZSBuZXcgbmFtZSBvZiB0aGUgcHJvamVjdFxuICAgICAqL1xuICAgIG5hbWU/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogVGhlIG5ldyBiaW5hcnkgbmFtZSAqLm5vZGUgZmlsZXNcbiAgICAgKi9cbiAgICBiaW5hcnlOYW1lPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFRoZSBuZXcgcGFja2FnZSBuYW1lIG9mIHRoZSBwcm9qZWN0XG4gICAgICovXG4gICAgcGFja2FnZU5hbWU/OiBzdHJpbmc7XG4gICAgbWFuaWZlc3RQYXRoOiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogVGhlIG5ldyByZXBvc2l0b3J5IG9mIHRoZSBwcm9qZWN0XG4gICAgICovXG4gICAgcmVwb3NpdG9yeT86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBUaGUgbmV3IGRlc2NyaXB0aW9uIG9mIHRoZSBwcm9qZWN0XG4gICAgICovXG4gICAgZGVzY3JpcHRpb24/OiBzdHJpbmc7XG59O1xuIiwiaW1wb3J0IHsgdHlwZSBSZW5hbWVPcHRpb25zIH0gZnJvbSAnLi4vZGVmL3JlbmFtZS5qcyc7XG5leHBvcnQgZGVjbGFyZSBmdW5jdGlvbiByZW5hbWVQcm9qZWN0KHVzZXJPcHRpb25zOiBSZW5hbWVPcHRpb25zKTogUHJvbWlzZTx2b2lkPjtcbiIsImltcG9ydCB7IENvbW1hbmQgfSBmcm9tICdjbGlwYW5pb24nO1xuZXhwb3J0IGRlY2xhcmUgYWJzdHJhY3QgY2xhc3MgQmFzZVVuaXZlcnNhbGl6ZUNvbW1hbmQgZXh0ZW5kcyBDb21tYW5kIHtcbiAgICBzdGF0aWMgcGF0aHM6IHN0cmluZ1tdW107XG4gICAgc3RhdGljIHVzYWdlOiBpbXBvcnQoXCJjbGlwYW5pb25cIikuVXNhZ2U7XG4gICAgY3dkOiBzdHJpbmc7XG4gICAgY29uZmlnUGF0aD86IHN0cmluZztcbiAgICBwYWNrYWdlSnNvblBhdGg6IHN0cmluZztcbiAgICBvdXRwdXREaXI6IHN0cmluZztcbiAgICBnZXRPcHRpb25zKCk6IHtcbiAgICAgICAgY3dkOiBzdHJpbmc7XG4gICAgICAgIGNvbmZpZ1BhdGg6IHN0cmluZyB8IHVuZGVmaW5lZDtcbiAgICAgICAgcGFja2FnZUpzb25QYXRoOiBzdHJpbmc7XG4gICAgICAgIG91dHB1dERpcjogc3RyaW5nO1xuICAgIH07XG59XG4vKipcbiAqIENvbWJpbGUgYnVpbHQgYmluYXJpZXMgaW50byBvbmUgdW5pdmVyc2FsIGJpbmFyeVxuICovXG5leHBvcnQgaW50ZXJmYWNlIFVuaXZlcnNhbGl6ZU9wdGlvbnMge1xuICAgIC8qKlxuICAgICAqIFRoZSB3b3JraW5nIGRpcmVjdG9yeSBvZiB3aGVyZSBuYXBpIGNvbW1hbmQgd2lsbCBiZSBleGVjdXRlZCBpbiwgYWxsIG90aGVyIHBhdGhzIG9wdGlvbnMgYXJlIHJlbGF0aXZlIHRvIHRoaXMgcGF0aFxuICAgICAqXG4gICAgICogQGRlZmF1bHQgcHJvY2Vzcy5jd2QoKVxuICAgICAqL1xuICAgIGN3ZD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIGBuYXBpYCBjb25maWcganNvbiBmaWxlXG4gICAgICovXG4gICAgY29uZmlnUGF0aD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIGBwYWNrYWdlLmpzb25gXG4gICAgICpcbiAgICAgKiBAZGVmYXVsdCAncGFja2FnZS5qc29uJ1xuICAgICAqL1xuICAgIHBhY2thZ2VKc29uUGF0aD86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIHRoZSBmb2xkZXIgd2hlcmUgYWxsIGJ1aWx0IGAubm9kZWAgZmlsZXMgcHV0LCBzYW1lIGFzIGAtLW91dHB1dC1kaXJgIG9mIGJ1aWxkIGNvbW1hbmRcbiAgICAgKlxuICAgICAqIEBkZWZhdWx0ICcuLydcbiAgICAgKi9cbiAgICBvdXRwdXREaXI/OiBzdHJpbmc7XG59XG5leHBvcnQgZGVjbGFyZSBmdW5jdGlvbiBhcHBseURlZmF1bHRVbml2ZXJzYWxpemVPcHRpb25zKG9wdGlvbnM6IFVuaXZlcnNhbGl6ZU9wdGlvbnMpOiB7XG4gICAgY3dkOiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogUGF0aCB0byBgbmFwaWAgY29uZmlnIGpzb24gZmlsZVxuICAgICAqL1xuICAgIGNvbmZpZ1BhdGg/OiBzdHJpbmc7XG4gICAgcGFja2FnZUpzb25QYXRoOiBzdHJpbmc7XG4gICAgb3V0cHV0RGlyOiBzdHJpbmc7XG59O1xuIiwiaW1wb3J0IHsgdHlwZSBVbml2ZXJzYWxpemVPcHRpb25zIH0gZnJvbSAnLi4vZGVmL3VuaXZlcnNhbGl6ZS5qcyc7XG5leHBvcnQgZGVjbGFyZSBmdW5jdGlvbiB1bml2ZXJzYWxpemVCaW5hcmllcyh1c2VyT3B0aW9uczogVW5pdmVyc2FsaXplT3B0aW9ucyk6IFByb21pc2U8dm9pZD47XG4iLCJpbXBvcnQgeyBDb21tYW5kIH0gZnJvbSAnY2xpcGFuaW9uJztcbmV4cG9ydCBkZWNsYXJlIGFic3RyYWN0IGNsYXNzIEJhc2VWZXJzaW9uQ29tbWFuZCBleHRlbmRzIENvbW1hbmQge1xuICAgIHN0YXRpYyBwYXRoczogc3RyaW5nW11bXTtcbiAgICBzdGF0aWMgdXNhZ2U6IGltcG9ydChcImNsaXBhbmlvblwiKS5Vc2FnZTtcbiAgICBjd2Q6IHN0cmluZztcbiAgICBjb25maWdQYXRoPzogc3RyaW5nO1xuICAgIHBhY2thZ2VKc29uUGF0aDogc3RyaW5nO1xuICAgIG5wbURpcjogc3RyaW5nO1xuICAgIGdldE9wdGlvbnMoKToge1xuICAgICAgICBjd2Q6IHN0cmluZztcbiAgICAgICAgY29uZmlnUGF0aDogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgICAgICBwYWNrYWdlSnNvblBhdGg6IHN0cmluZztcbiAgICAgICAgbnBtRGlyOiBzdHJpbmc7XG4gICAgfTtcbn1cbi8qKlxuICogVXBkYXRlIHZlcnNpb24gaW4gY3JlYXRlZCBucG0gcGFja2FnZXNcbiAqL1xuZXhwb3J0IGludGVyZmFjZSBWZXJzaW9uT3B0aW9ucyB7XG4gICAgLyoqXG4gICAgICogVGhlIHdvcmtpbmcgZGlyZWN0b3J5IG9mIHdoZXJlIG5hcGkgY29tbWFuZCB3aWxsIGJlIGV4ZWN1dGVkIGluLCBhbGwgb3RoZXIgcGF0aHMgb3B0aW9ucyBhcmUgcmVsYXRpdmUgdG8gdGhpcyBwYXRoXG4gICAgICpcbiAgICAgKiBAZGVmYXVsdCBwcm9jZXNzLmN3ZCgpXG4gICAgICovXG4gICAgY3dkPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gYG5hcGlgIGNvbmZpZyBqc29uIGZpbGVcbiAgICAgKi9cbiAgICBjb25maWdQYXRoPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gYHBhY2thZ2UuanNvbmBcbiAgICAgKlxuICAgICAqIEBkZWZhdWx0ICdwYWNrYWdlLmpzb24nXG4gICAgICovXG4gICAgcGFja2FnZUpzb25QYXRoPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFBhdGggdG8gdGhlIGZvbGRlciB3aGVyZSB0aGUgbnBtIHBhY2thZ2VzIHB1dFxuICAgICAqXG4gICAgICogQGRlZmF1bHQgJ25wbSdcbiAgICAgKi9cbiAgICBucG1EaXI/OiBzdHJpbmc7XG59XG5leHBvcnQgZGVjbGFyZSBmdW5jdGlvbiBhcHBseURlZmF1bHRWZXJzaW9uT3B0aW9ucyhvcHRpb25zOiBWZXJzaW9uT3B0aW9ucyk6IHtcbiAgICBjd2Q6IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBQYXRoIHRvIGBuYXBpYCBjb25maWcganNvbiBmaWxlXG4gICAgICovXG4gICAgY29uZmlnUGF0aD86IHN0cmluZztcbiAgICBwYWNrYWdlSnNvblBhdGg6IHN0cmluZztcbiAgICBucG1EaXI6IHN0cmluZztcbn07XG4iLCJpbXBvcnQgeyB0eXBlIFZlcnNpb25PcHRpb25zIH0gZnJvbSAnLi4vZGVmL3ZlcnNpb24uanMnO1xuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gdmVyc2lvbih1c2VyT3B0aW9uczogVmVyc2lvbk9wdGlvbnMpOiBQcm9taXNlPHZvaWQ+O1xuIiwiaW1wb3J0IHsgQmFzZUFydGlmYWN0c0NvbW1hbmQgfSBmcm9tICcuLi9kZWYvYXJ0aWZhY3RzLmpzJztcbmV4cG9ydCBkZWNsYXJlIGNsYXNzIEFydGlmYWN0c0NvbW1hbmQgZXh0ZW5kcyBCYXNlQXJ0aWZhY3RzQ29tbWFuZCB7XG4gICAgc3RhdGljIHVzYWdlOiBpbXBvcnQoXCJjbGlwYW5pb25cIikuVXNhZ2U7XG4gICAgc3RhdGljIHBhdGhzOiBzdHJpbmdbXVtdO1xuICAgIGV4ZWN1dGUoKTogUHJvbWlzZTx2b2lkPjtcbn1cbiIsImltcG9ydCB7IEJhc2VCdWlsZENvbW1hbmQgfSBmcm9tICcuLi9kZWYvYnVpbGQuanMnO1xuZXhwb3J0IGRlY2xhcmUgY2xhc3MgQnVpbGRDb21tYW5kIGV4dGVuZHMgQmFzZUJ1aWxkQ29tbWFuZCB7XG4gICAgcGlwZTogc3RyaW5nIHwgdW5kZWZpbmVkO1xuICAgIGNhcmdvT3B0aW9uczogc3RyaW5nW107XG4gICAgZXhlY3V0ZSgpOiBQcm9taXNlPHZvaWQ+O1xufVxuIiwiaW1wb3J0IHsgQmFzZUNyZWF0ZU5wbURpcnNDb21tYW5kIH0gZnJvbSAnLi4vZGVmL2NyZWF0ZS1ucG0tZGlycy5qcyc7XG5leHBvcnQgZGVjbGFyZSBjbGFzcyBDcmVhdGVOcG1EaXJzQ29tbWFuZCBleHRlbmRzIEJhc2VDcmVhdGVOcG1EaXJzQ29tbWFuZCB7XG4gICAgZXhlY3V0ZSgpOiBQcm9taXNlPHZvaWQ+O1xufVxuIiwiaW1wb3J0IHsgQmFzZU5ld0NvbW1hbmQgfSBmcm9tICcuLi9kZWYvbmV3LmpzJztcbmV4cG9ydCBkZWNsYXJlIGNsYXNzIE5ld0NvbW1hbmQgZXh0ZW5kcyBCYXNlTmV3Q29tbWFuZCB7XG4gICAgaW50ZXJhY3RpdmU6IGJvb2xlYW47XG4gICAgZXhlY3V0ZSgpOiBQcm9taXNlPDAgfCAxPjtcbiAgICBwcml2YXRlIGZldGNoT3B0aW9ucztcbiAgICBwcml2YXRlIGZldGNoTmFtZTtcbiAgICBwcml2YXRlIGZldGNoTGljZW5zZTtcbiAgICBwcml2YXRlIGZldGNoTmFwaVZlcnNpb247XG4gICAgcHJpdmF0ZSBmZXRjaFRhcmdldHM7XG4gICAgcHJpdmF0ZSBmZXRjaFR5cGVEZWY7XG4gICAgcHJpdmF0ZSBmZXRjaEdpdGh1YkFjdGlvbnM7XG59XG4iLCJpbXBvcnQgeyBCYXNlUHJlUHVibGlzaENvbW1hbmQgfSBmcm9tICcuLi9kZWYvcHJlLXB1Ymxpc2guanMnO1xuZXhwb3J0IGRlY2xhcmUgY2xhc3MgUHJlUHVibGlzaENvbW1hbmQgZXh0ZW5kcyBCYXNlUHJlUHVibGlzaENvbW1hbmQge1xuICAgIGV4ZWN1dGUoKTogUHJvbWlzZTx2b2lkPjtcbn1cbiIsImltcG9ydCB7IEJhc2VSZW5hbWVDb21tYW5kIH0gZnJvbSAnLi4vZGVmL3JlbmFtZS5qcyc7XG5leHBvcnQgZGVjbGFyZSBjbGFzcyBSZW5hbWVDb21tYW5kIGV4dGVuZHMgQmFzZVJlbmFtZUNvbW1hbmQge1xuICAgIGV4ZWN1dGUoKTogUHJvbWlzZTx2b2lkPjtcbn1cbiIsImltcG9ydCB7IEJhc2VVbml2ZXJzYWxpemVDb21tYW5kIH0gZnJvbSAnLi4vZGVmL3VuaXZlcnNhbGl6ZS5qcyc7XG5leHBvcnQgZGVjbGFyZSBjbGFzcyBVbml2ZXJzYWxpemVDb21tYW5kIGV4dGVuZHMgQmFzZVVuaXZlcnNhbGl6ZUNvbW1hbmQge1xuICAgIGV4ZWN1dGUoKTogUHJvbWlzZTx2b2lkPjtcbn1cbiIsImltcG9ydCB7IEJhc2VWZXJzaW9uQ29tbWFuZCB9IGZyb20gJy4uL2RlZi92ZXJzaW9uLmpzJztcbmV4cG9ydCBkZWNsYXJlIGNsYXNzIFZlcnNpb25Db21tYW5kIGV4dGVuZHMgQmFzZVZlcnNpb25Db21tYW5kIHtcbiAgICBleGVjdXRlKCk6IFByb21pc2U8dm9pZD47XG59XG4iLCJleHBvcnQgdHlwZSBQbGF0Zm9ybSA9IE5vZGVKUy5QbGF0Zm9ybSB8ICd3YXNtJyB8ICd3YXNpJyB8ICdvcGVuaGFybW9ueSc7XG5leHBvcnQgZGVjbGFyZSBjb25zdCBVTklWRVJTQUxfVEFSR0VUUzoge1xuICAgIHJlYWRvbmx5ICd1bml2ZXJzYWwtYXBwbGUtZGFyd2luJzogcmVhZG9ubHkgW1wiYWFyY2g2NC1hcHBsZS1kYXJ3aW5cIiwgXCJ4ODZfNjQtYXBwbGUtZGFyd2luXCJdO1xufTtcbmV4cG9ydCBkZWNsYXJlIGNvbnN0IEFWQUlMQUJMRV9UQVJHRVRTOiByZWFkb25seSBbXCJhYXJjaDY0LWFwcGxlLWRhcndpblwiLCBcImFhcmNoNjQtbGludXgtYW5kcm9pZFwiLCBcImFhcmNoNjQtdW5rbm93bi1saW51eC1nbnVcIiwgXCJhYXJjaDY0LXVua25vd24tbGludXgtbXVzbFwiLCBcImFhcmNoNjQtdW5rbm93bi1saW51eC1vaG9zXCIsIFwiYWFyY2g2NC1wYy13aW5kb3dzLW1zdmNcIiwgXCJ4ODZfNjQtYXBwbGUtZGFyd2luXCIsIFwieDg2XzY0LXBjLXdpbmRvd3MtbXN2Y1wiLCBcIng4Nl82NC1wYy13aW5kb3dzLWdudVwiLCBcIng4Nl82NC11bmtub3duLWxpbnV4LWdudVwiLCBcIng4Nl82NC11bmtub3duLWxpbnV4LW11c2xcIiwgXCJ4ODZfNjQtdW5rbm93bi1saW51eC1vaG9zXCIsIFwieDg2XzY0LXVua25vd24tZnJlZWJzZFwiLCBcImk2ODYtcGMtd2luZG93cy1tc3ZjXCIsIFwiYXJtdjctdW5rbm93bi1saW51eC1nbnVlYWJpaGZcIiwgXCJhcm12Ny11bmtub3duLWxpbnV4LW11c2xlYWJpaGZcIiwgXCJhcm12Ny1saW51eC1hbmRyb2lkZWFiaVwiLCBcInVuaXZlcnNhbC1hcHBsZS1kYXJ3aW5cIiwgXCJsb29uZ2FyY2g2NC11bmtub3duLWxpbnV4LWdudVwiLCBcInJpc2N2NjRnYy11bmtub3duLWxpbnV4LWdudVwiLCBcInBvd2VycGM2NGxlLXVua25vd24tbGludXgtZ251XCIsIFwiczM5MHgtdW5rbm93bi1saW51eC1nbnVcIiwgXCJ3YXNtMzItd2FzaS1wcmV2aWV3MS10aHJlYWRzXCIsIFwid2FzbTMyLXdhc2lwMS10aHJlYWRzXCJdO1xuZXhwb3J0IHR5cGUgVGFyZ2V0VHJpcGxlID0gKHR5cGVvZiBBVkFJTEFCTEVfVEFSR0VUUylbbnVtYmVyXTtcbmV4cG9ydCBkZWNsYXJlIGNvbnN0IERFRkFVTFRfVEFSR0VUUzogcmVhZG9ubHkgW1wieDg2XzY0LWFwcGxlLWRhcndpblwiLCBcImFhcmNoNjQtYXBwbGUtZGFyd2luXCIsIFwieDg2XzY0LXBjLXdpbmRvd3MtbXN2Y1wiLCBcIng4Nl82NC11bmtub3duLWxpbnV4LWdudVwiXTtcbmV4cG9ydCBkZWNsYXJlIGNvbnN0IFRBUkdFVF9MSU5LRVI6IFJlY29yZDxzdHJpbmcsIHN0cmluZz47XG50eXBlIE5vZGVKU0FyY2ggPSAnYXJtJyB8ICdhcm02NCcgfCAnaWEzMicgfCAnbG9vbmc2NCcgfCAnbWlwcycgfCAnbWlwc2VsJyB8ICdwcGMnIHwgJ3BwYzY0JyB8ICdyaXNjdjY0JyB8ICdzMzkwJyB8ICdzMzkweCcgfCAneDMyJyB8ICd4NjQnIHwgJ3VuaXZlcnNhbCcgfCAnd2FzbTMyJztcbmV4cG9ydCBkZWNsYXJlIGNvbnN0IE5vZGVBcmNoVG9DcHU6IFJlY29yZDxzdHJpbmcsIHN0cmluZz47XG5leHBvcnQgZGVjbGFyZSBjb25zdCBVbmlBcmNoc0J5UGxhdGZvcm06IFBhcnRpYWw8UmVjb3JkPFBsYXRmb3JtLCBOb2RlSlNBcmNoW10+PjtcbmV4cG9ydCBpbnRlcmZhY2UgVGFyZ2V0IHtcbiAgICB0cmlwbGU6IHN0cmluZztcbiAgICBwbGF0Zm9ybUFyY2hBQkk6IHN0cmluZztcbiAgICBwbGF0Zm9ybTogUGxhdGZvcm07XG4gICAgYXJjaDogTm9kZUpTQXJjaDtcbiAgICBhYmk6IHN0cmluZyB8IG51bGw7XG59XG4vKipcbiAqIEEgdHJpcGxlIGlzIGEgc3BlY2lmaWMgZm9ybWF0IGZvciBzcGVjaWZ5aW5nIGEgdGFyZ2V0IGFyY2hpdGVjdHVyZS5cbiAqIFRyaXBsZXMgbWF5IGJlIHJlZmVycmVkIHRvIGFzIGEgdGFyZ2V0IHRyaXBsZSB3aGljaCBpcyB0aGUgYXJjaGl0ZWN0dXJlIGZvciB0aGUgYXJ0aWZhY3QgcHJvZHVjZWQsIGFuZCB0aGUgaG9zdCB0cmlwbGUgd2hpY2ggaXMgdGhlIGFyY2hpdGVjdHVyZSB0aGF0IHRoZSBjb21waWxlciBpcyBydW5uaW5nIG9uLlxuICogVGhlIGdlbmVyYWwgZm9ybWF0IG9mIHRoZSB0cmlwbGUgaXMgYDxhcmNoPjxzdWI+LTx2ZW5kb3I+LTxzeXM+LTxhYmk+YCB3aGVyZTpcbiAqICAgLSBgYXJjaGAgPSBUaGUgYmFzZSBDUFUgYXJjaGl0ZWN0dXJlLCBmb3IgZXhhbXBsZSBgeDg2XzY0YCwgYGk2ODZgLCBgYXJtYCwgYHRodW1iYCwgYG1pcHNgLCBldGMuXG4gKiAgIC0gYHN1YmAgPSBUaGUgQ1BVIHN1Yi1hcmNoaXRlY3R1cmUsIGZvciBleGFtcGxlIGBhcm1gIGhhcyBgdjdgLCBgdjdzYCwgYHY1dGVgLCBldGMuXG4gKiAgIC0gYHZlbmRvcmAgPSBUaGUgdmVuZG9yLCBmb3IgZXhhbXBsZSBgdW5rbm93bmAsIGBhcHBsZWAsIGBwY2AsIGBudmlkaWFgLCBldGMuXG4gKiAgIC0gYHN5c2AgPSBUaGUgc3lzdGVtIG5hbWUsIGZvciBleGFtcGxlIGBsaW51eGAsIGB3aW5kb3dzYCwgYGRhcndpbmAsIGV0Yy4gbm9uZSBpcyB0eXBpY2FsbHkgdXNlZCBmb3IgYmFyZS1tZXRhbCB3aXRob3V0IGFuIE9TLlxuICogICAtIGBhYmlgID0gVGhlIEFCSSwgZm9yIGV4YW1wbGUgYGdudWAsIGBhbmRyb2lkYCwgYGVhYmlgLCBldGMuXG4gKi9cbmV4cG9ydCBkZWNsYXJlIGZ1bmN0aW9uIHBhcnNlVHJpcGxlKHJhd1RyaXBsZTogc3RyaW5nKTogVGFyZ2V0O1xuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gZ2V0U3lzdGVtRGVmYXVsdFRhcmdldCgpOiBUYXJnZXQ7XG5leHBvcnQgZGVjbGFyZSBmdW5jdGlvbiBnZXRUYXJnZXRMaW5rZXIodGFyZ2V0OiBzdHJpbmcpOiBzdHJpbmcgfCB1bmRlZmluZWQ7XG5leHBvcnQgZGVjbGFyZSBmdW5jdGlvbiB0YXJnZXRUb0VudlZhcih0YXJnZXQ6IHN0cmluZyk6IHN0cmluZztcbmV4cG9ydCB7fTtcbiIsImltcG9ydCB7IHR5cGUgVGFyZ2V0IH0gZnJvbSAnLi90YXJnZXQuanMnO1xuZXhwb3J0IHR5cGUgVmFsdWVPZkNvbnN0QXJyYXk8VD4gPSBUW0V4Y2x1ZGU8a2V5b2YgVCwga2V5b2YgQXJyYXk8YW55Pj5dO1xuZXhwb3J0IGRlY2xhcmUgY29uc3QgU3VwcG9ydGVkUGFja2FnZU1hbmFnZXJzOiByZWFkb25seSBbXCJ5YXJuXCIsIFwicG5wbVwiXTtcbmV4cG9ydCBkZWNsYXJlIGNvbnN0IFN1cHBvcnRlZFRlc3RGcmFtZXdvcmtzOiByZWFkb25seSBbXCJhdmFcIl07XG5leHBvcnQgdHlwZSBTdXBwb3J0ZWRQYWNrYWdlTWFuYWdlciA9IFZhbHVlT2ZDb25zdEFycmF5PHR5cGVvZiBTdXBwb3J0ZWRQYWNrYWdlTWFuYWdlcnM+O1xuZXhwb3J0IHR5cGUgU3VwcG9ydGVkVGVzdEZyYW1ld29yayA9IFZhbHVlT2ZDb25zdEFycmF5PHR5cGVvZiBTdXBwb3J0ZWRUZXN0RnJhbWV3b3Jrcz47XG5leHBvcnQgaW50ZXJmYWNlIFVzZXJOYXBpQ29uZmlnIHtcbiAgICAvKipcbiAgICAgKiBOYW1lIG9mIHRoZSBiaW5hcnkgdG8gYmUgZ2VuZXJhdGVkLCBkZWZhdWx0IHRvIGBpbmRleGBcbiAgICAgKi9cbiAgICBiaW5hcnlOYW1lPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIE5hbWUgb2YgdGhlIG5wbSBwYWNrYWdlLCBkZWZhdWx0IHRvIHRoZSBuYW1lIG9mIHJvb3QgcGFja2FnZS5qc29uIG5hbWVcbiAgICAgKlxuICAgICAqIEFsd2F5cyBnaXZlbiBgQHNjb3BlL3BrZ2AgYW5kIGFyY2ggc3VmZml4IHdpbGwgYmUgYXBwZW5kZWQgbGlrZSBgQHNjb3BlL3BrZy1saW51eC1nbnUteDY0YFxuICAgICAqL1xuICAgIHBhY2thZ2VOYW1lPzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIEFsbCB0YXJnZXRzIHRoZSBjcmF0ZSB3aWxsIGJlIGNvbXBpbGVkIGZvclxuICAgICAqL1xuICAgIHRhcmdldHM/OiBzdHJpbmdbXTtcbiAgICAvKipcbiAgICAgKiBUaGUgbnBtIGNsaWVudCBwcm9qZWN0IHVzZXMuXG4gICAgICovXG4gICAgbnBtQ2xpZW50Pzogc3RyaW5nO1xuICAgIC8qKlxuICAgICAqIFdoZXRoZXIgZ2VuZXJhdGUgY29uc3QgZW51bSBmb3IgdHlwZXNjcmlwdCBiaW5kaW5nc1xuICAgICAqL1xuICAgIGNvbnN0RW51bT86IGJvb2xlYW47XG4gICAgLyoqXG4gICAgICogZHRzIGhlYWRlciBwcmVwZW5kIHRvIHRoZSBnZW5lcmF0ZWQgZHRzIGZpbGVcbiAgICAgKi9cbiAgICBkdHNIZWFkZXI/OiBzdHJpbmc7XG4gICAgLyoqXG4gICAgICogZHRzIGhlYWRlciBmaWxlIHBhdGggdG8gYmUgcHJlcGVuZGVkIHRvIHRoZSBnZW5lcmF0ZWQgZHRzIGZpbGVcbiAgICAgKiBpZiBib3RoIGR0c0hlYWRlciBhbmQgZHRzSGVhZGVyRmlsZSBhcmUgcHJvdmlkZWQsIGR0c0hlYWRlckZpbGUgd2lsbCBiZSB1c2VkXG4gICAgICovXG4gICAgZHRzSGVhZGVyRmlsZT86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiB3YXNtIGNvbXBpbGF0aW9uIG9wdGlvbnNcbiAgICAgKi9cbiAgICB3YXNtPzoge1xuICAgICAgICAvKipcbiAgICAgICAgICogaHR0cHM6Ly9kZXZlbG9wZXIubW96aWxsYS5vcmcvZW4tVVMvZG9jcy9XZWJBc3NlbWJseS9KYXZhU2NyaXB0X2ludGVyZmFjZS9NZW1vcnlcbiAgICAgICAgICogQGRlZmF1bHQgNDAwMCBwYWdlcyAoMjU2TWlCKVxuICAgICAgICAgKi9cbiAgICAgICAgaW5pdGlhbE1lbW9yeT86IG51bWJlcjtcbiAgICAgICAgLyoqXG4gICAgICAgICAqIEBkZWZhdWx0IDY1NTM2IHBhZ2VzICg0R2lCKVxuICAgICAgICAgKi9cbiAgICAgICAgbWF4aW11bU1lbW9yeT86IG51bWJlcjtcbiAgICAgICAgLyoqXG4gICAgICAgICAqIEJyb3dzZXIgd2FzbSBiaW5kaW5nIGNvbmZpZ3VyYXRpb25cbiAgICAgICAgICovXG4gICAgICAgIGJyb3dzZXI6IHtcbiAgICAgICAgICAgIC8qKlxuICAgICAgICAgICAgICogV2hldGhlciB0byB1c2UgZnMgbW9kdWxlIGluIGJyb3dzZXJcbiAgICAgICAgICAgICAqL1xuICAgICAgICAgICAgZnM/OiBib29sZWFuO1xuICAgICAgICAgICAgLyoqXG4gICAgICAgICAgICAgKiBXaGV0aGVyIHRvIGluaXRpYWxpemUgd2FzbSBhc3luY2hyb25vdXNseVxuICAgICAgICAgICAgICovXG4gICAgICAgICAgICBhc3luY0luaXQ/OiBib29sZWFuO1xuICAgICAgICAgICAgLyoqXG4gICAgICAgICAgICAgKiBXaGV0aGVyIHRvIGluamVjdCBgYnVmZmVyYCB0byBlbW5hcGkgY29udGV4dFxuICAgICAgICAgICAgICovXG4gICAgICAgICAgICBidWZmZXI/OiBib29sZWFuO1xuICAgICAgICB9O1xuICAgIH07XG4gICAgLyoqXG4gICAgICogQGRlcHJlY2F0ZWQgYmluYXJ5TmFtZSBpbnN0ZWFkXG4gICAgICovXG4gICAgbmFtZT86IHN0cmluZztcbiAgICAvKipcbiAgICAgKiBAZGVwcmVjYXRlZCB1c2UgcGFja2FnZU5hbWUgaW5zdGVhZFxuICAgICAqL1xuICAgIHBhY2thZ2U/OiB7XG4gICAgICAgIG5hbWU/OiBzdHJpbmc7XG4gICAgfTtcbiAgICAvKipcbiAgICAgKiBAZGVwcmVjYXRlZCB1c2UgdGFyZ2V0cyBpbnN0ZWFkXG4gICAgICovXG4gICAgdHJpcGxlcz86IHtcbiAgICAgICAgLyoqXG4gICAgICAgICAqIFdoZXRoZXIgZW5hYmxlIGRlZmF1bHQgdGFyZ2V0c1xuICAgICAgICAgKi9cbiAgICAgICAgZGVmYXVsdHM6IGJvb2xlYW47XG4gICAgICAgIC8qKlxuICAgICAgICAgKiBBZGRpdGlvbmFsIHRhcmdldHMgdG8gYmUgY29tcGlsZWQgZm9yXG4gICAgICAgICAqL1xuICAgICAgICBhZGRpdGlvbmFsPzogc3RyaW5nW107XG4gICAgfTtcbn1cbmV4cG9ydCBpbnRlcmZhY2UgQ29tbW9uUGFja2FnZUpzb25GaWVsZHMge1xuICAgIG5hbWU6IHN0cmluZztcbiAgICB2ZXJzaW9uOiBzdHJpbmc7XG4gICAgZGVzY3JpcHRpb24/OiBzdHJpbmc7XG4gICAga2V5d29yZHM/OiBzdHJpbmdbXTtcbiAgICBhdXRob3I/OiBzdHJpbmc7XG4gICAgYXV0aG9ycz86IHN0cmluZ1tdO1xuICAgIGxpY2Vuc2U/OiBzdHJpbmc7XG4gICAgY3B1Pzogc3RyaW5nW107XG4gICAgb3M/OiBzdHJpbmdbXTtcbiAgICBsaWJjPzogc3RyaW5nW107XG4gICAgZmlsZXM/OiBzdHJpbmdbXTtcbiAgICByZXBvc2l0b3J5PzogYW55O1xuICAgIGhvbWVwYWdlPzogYW55O1xuICAgIGVuZ2luZXM/OiBSZWNvcmQ8c3RyaW5nLCBzdHJpbmc+O1xuICAgIHB1Ymxpc2hDb25maWc/OiBhbnk7XG4gICAgYnVncz86IGFueTtcbiAgICBuYXBpPzogVXNlck5hcGlDb25maWc7XG4gICAgdHlwZT86ICdtb2R1bGUnIHwgJ2NvbW1vbmpzJztcbiAgICBzY3JpcHRzPzogUmVjb3JkPHN0cmluZywgc3RyaW5nPjtcbiAgICBtYWluPzogc3RyaW5nO1xuICAgIG1vZHVsZT86IHN0cmluZztcbiAgICB0eXBlcz86IHN0cmluZztcbiAgICBicm93c2VyPzogc3RyaW5nO1xuICAgIGV4cG9ydHM/OiBhbnk7XG4gICAgZGVwZW5kZW5jaWVzPzogUmVjb3JkPHN0cmluZywgc3RyaW5nPjtcbiAgICBkZXZEZXBlbmRlbmNpZXM/OiBSZWNvcmQ8c3RyaW5nLCBzdHJpbmc+O1xuICAgIGF2YT86IHtcbiAgICAgICAgdGltZW91dD86IHN0cmluZztcbiAgICB9O1xufVxuZXhwb3J0IHR5cGUgTmFwaUNvbmZpZyA9IFJlcXVpcmVkPFBpY2s8VXNlck5hcGlDb25maWcsICdiaW5hcnlOYW1lJyB8ICdwYWNrYWdlTmFtZScgfCAnbnBtQ2xpZW50Jz4+ICYgUGljazxVc2VyTmFwaUNvbmZpZywgJ3dhc20nIHwgJ2R0c0hlYWRlcicgfCAnZHRzSGVhZGVyRmlsZScgfCAnY29uc3RFbnVtJz4gJiB7XG4gICAgdGFyZ2V0czogVGFyZ2V0W107XG4gICAgcGFja2FnZUpzb246IENvbW1vblBhY2thZ2VKc29uRmllbGRzO1xufTtcbmV4cG9ydCBkZWNsYXJlIGZ1bmN0aW9uIHJlYWROYXBpQ29uZmlnKHBhdGg6IHN0cmluZywgY29uZmlnUGF0aD86IHN0cmluZyk6IFByb21pc2U8TmFwaUNvbmZpZz47XG4iLCJpbXBvcnQgeyBDbGkgfSBmcm9tICdjbGlwYW5pb24nO1xuaW1wb3J0IHsgY29sbGVjdEFydGlmYWN0cyB9IGZyb20gJy4vYXBpL2FydGlmYWN0cy5qcyc7XG5pbXBvcnQgeyBidWlsZFByb2plY3QgfSBmcm9tICcuL2FwaS9idWlsZC5qcyc7XG5pbXBvcnQgeyBjcmVhdGVOcG1EaXJzIH0gZnJvbSAnLi9hcGkvY3JlYXRlLW5wbS1kaXJzLmpzJztcbmltcG9ydCB7IG5ld1Byb2plY3QgfSBmcm9tICcuL2FwaS9uZXcuanMnO1xuaW1wb3J0IHsgcHJlUHVibGlzaCB9IGZyb20gJy4vYXBpL3ByZS1wdWJsaXNoLmpzJztcbmltcG9ydCB7IHJlbmFtZVByb2plY3QgfSBmcm9tICcuL2FwaS9yZW5hbWUuanMnO1xuaW1wb3J0IHsgdW5pdmVyc2FsaXplQmluYXJpZXMgfSBmcm9tICcuL2FwaS91bml2ZXJzYWxpemUuanMnO1xuaW1wb3J0IHsgdmVyc2lvbiB9IGZyb20gJy4vYXBpL3ZlcnNpb24uanMnO1xuaW1wb3J0IHsgQXJ0aWZhY3RzQ29tbWFuZCB9IGZyb20gJy4vY29tbWFuZHMvYXJ0aWZhY3RzLmpzJztcbmltcG9ydCB7IEJ1aWxkQ29tbWFuZCB9IGZyb20gJy4vY29tbWFuZHMvYnVpbGQuanMnO1xuaW1wb3J0IHsgQ3JlYXRlTnBtRGlyc0NvbW1hbmQgfSBmcm9tICcuL2NvbW1hbmRzL2NyZWF0ZS1ucG0tZGlycy5qcyc7XG5pbXBvcnQgeyBOZXdDb21tYW5kIH0gZnJvbSAnLi9jb21tYW5kcy9uZXcuanMnO1xuaW1wb3J0IHsgUHJlUHVibGlzaENvbW1hbmQgfSBmcm9tICcuL2NvbW1hbmRzL3ByZS1wdWJsaXNoLmpzJztcbmltcG9ydCB7IFJlbmFtZUNvbW1hbmQgfSBmcm9tICcuL2NvbW1hbmRzL3JlbmFtZS5qcyc7XG5pbXBvcnQgeyBVbml2ZXJzYWxpemVDb21tYW5kIH0gZnJvbSAnLi9jb21tYW5kcy91bml2ZXJzYWxpemUuanMnO1xuaW1wb3J0IHsgVmVyc2lvbkNvbW1hbmQgfSBmcm9tICcuL2NvbW1hbmRzL3ZlcnNpb24uanMnO1xuZXhwb3J0IGRlY2xhcmUgY29uc3QgY2xpOiBDbGk8aW1wb3J0KFwiY2xpcGFuaW9uXCIpLkJhc2VDb250ZXh0Pjtcbi8qKlxuICpcbiAqIEB1c2FnZVxuICpcbiAqIGBgYHRzXG4gKiBjb25zdCBjbGkgPSBuZXcgTmFwaUNsaSgpXG4gKlxuICogY2xpLmJ1aWxkKHtcbiAqICAgY3dkOiAnL3BhdGgvdG8veW91ci9wcm9qZWN0JyxcbiAqIH0pXG4gKiBgYGBcbiAqL1xuZXhwb3J0IGRlY2xhcmUgY2xhc3MgTmFwaUNsaSB7XG4gICAgYXJ0aWZhY3RzOiB0eXBlb2YgY29sbGVjdEFydGlmYWN0cztcbiAgICBuZXc6IHR5cGVvZiBuZXdQcm9qZWN0O1xuICAgIGJ1aWxkOiB0eXBlb2YgYnVpbGRQcm9qZWN0O1xuICAgIGNyZWF0ZU5wbURpcnM6IHR5cGVvZiBjcmVhdGVOcG1EaXJzO1xuICAgIHByZVB1Ymxpc2g6IHR5cGVvZiBwcmVQdWJsaXNoO1xuICAgIHJlbmFtZTogdHlwZW9mIHJlbmFtZVByb2plY3Q7XG4gICAgdW5pdmVyc2FsaXplOiB0eXBlb2YgdW5pdmVyc2FsaXplQmluYXJpZXM7XG4gICAgdmVyc2lvbjogdHlwZW9mIHZlcnNpb247XG59XG5leHBvcnQgZGVjbGFyZSBmdW5jdGlvbiBjcmVhdGVCdWlsZENvbW1hbmQoYXJnczogc3RyaW5nW10pOiBCdWlsZENvbW1hbmQ7XG5leHBvcnQgZGVjbGFyZSBmdW5jdGlvbiBjcmVhdGVBcnRpZmFjdHNDb21tYW5kKGFyZ3M6IHN0cmluZ1tdKTogQXJ0aWZhY3RzQ29tbWFuZDtcbmV4cG9ydCBkZWNsYXJlIGZ1bmN0aW9uIGNyZWF0ZUNyZWF0ZU5wbURpcnNDb21tYW5kKGFyZ3M6IHN0cmluZ1tdKTogQ3JlYXRlTnBtRGlyc0NvbW1hbmQ7XG5leHBvcnQgZGVjbGFyZSBmdW5jdGlvbiBjcmVhdGVQcmVQdWJsaXNoQ29tbWFuZChhcmdzOiBzdHJpbmdbXSk6IFByZVB1Ymxpc2hDb21tYW5kO1xuZXhwb3J0IGRlY2xhcmUgZnVuY3Rpb24gY3JlYXRlUmVuYW1lQ29tbWFuZChhcmdzOiBzdHJpbmdbXSk6IFJlbmFtZUNvbW1hbmQ7XG5leHBvcnQgZGVjbGFyZSBmdW5jdGlvbiBjcmVhdGVVbml2ZXJzYWxpemVDb21tYW5kKGFyZ3M6IHN0cmluZ1tdKTogVW5pdmVyc2FsaXplQ29tbWFuZDtcbmV4cG9ydCBkZWNsYXJlIGZ1bmN0aW9uIGNyZWF0ZVZlcnNpb25Db21tYW5kKGFyZ3M6IHN0cmluZ1tdKTogVmVyc2lvbkNvbW1hbmQ7XG5leHBvcnQgZGVjbGFyZSBmdW5jdGlvbiBjcmVhdGVOZXdDb21tYW5kKGFyZ3M6IHN0cmluZ1tdKTogTmV3Q29tbWFuZDtcbmV4cG9ydCB7IHBhcnNlVHJpcGxlIH0gZnJvbSAnLi91dGlscy90YXJnZXQuanMnO1xuZXhwb3J0IHsgdHlwZSBHZW5lcmF0ZVR5cGVEZWZPcHRpb25zLCB0eXBlIFdyaXRlSnNCaW5kaW5nT3B0aW9ucywgd3JpdGVKc0JpbmRpbmcsIGdlbmVyYXRlVHlwZURlZiwgfSBmcm9tICcuL2FwaS9idWlsZC5qcyc7XG5l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