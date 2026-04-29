export const include: string
export const include_dir: string
export const js_library: string
export const sources: string[]
export const targets: string
export const requiredConfig: {
  emscripten: {
    settings: Record<string, string | string[]>
  }
  clang: {
    target: string
    cflags: string[]
    ldflags: string[]
    wasmld: string[]
  }
}
