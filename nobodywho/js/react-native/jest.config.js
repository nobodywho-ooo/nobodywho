module.exports = {
  preset: "ts-jest",
  testEnvironment: "node",
  roots: ["<rootDir>/__tests__"],
  modulePathIgnorePatterns: ["<rootDir>/test-app/"],
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
