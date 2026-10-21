module.exports = {
  preset: "ts-jest",
  testEnvironment: "node",
  roots: ["<rootDir>/__tests__"],
  // `*.test.mjs` are WASM integration tests, run by `node --test`.
  testMatch: ["**/*.test.ts"],
  modulePathIgnorePatterns: ["<rootDir>/test-app/"],
  // Resolve `./bindings` to `bindings.native.ts` like Metro does, so these
  // tests use the JSI bindings (CommonJS-friendly, and loading them doesn't
  // need the native library).
  haste: {
    defaultPlatform: "native",
    platforms: ["native"],
  },
  transform: {
    "^.+\\.tsx?$": [
      "ts-jest",
      {
        tsconfig: {
          types: ["jest"],
          lib: ["es2020"],
          target: "es2020",
          esModuleInterop: true,
          strict: false,
        },
      },
    ],
  },
};
