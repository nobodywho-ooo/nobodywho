export const arm64TargetX86_64: string | undefined
export const arm64TargetAarch64: string | undefined
export const arm64TargetArmv7: string | undefined
export const x64TargetX86_64: string | undefined
export const x64TargetAarch64: string | undefined
export const x64TargetArmv7: string | undefined
export type SupportedTargets =
  | 'armv7-unknown-linux-gnueabihf'
  | 'aarch64-unknown-linux-gnu'
  | 'x86_64-unknown-linux-gnu'
export type PlatformToolchain = {
  [key in SupportedTargets]: string | undefined
}
export const arm64: PlatformToolchain
export const x64: PlatformToolchain
export function download(
  arch: 'x64' | 'arm64',
  target: SupportedTargets
): import('@napi-rs/tar').Archive
