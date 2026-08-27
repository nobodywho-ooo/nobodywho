// @ts-check
//
// Expo config plugin for react-native-nobodywho.
//
// `react-native-nobodywho` is a standard React Native TurboModule package, so
// Expo autolinking already discovers and links it during `expo prebuild`. This
// plugin fills in the native build settings that autolinking does not manage:
//
//   • Android: raises `minSdkVersion` (default 31) so the x86_64 emulator — which
//     the Rust runtime requires API 31+ for (ELF TLS) — works out of the box.
//   • iOS: ensures the Podfile deployment target is at least iOS 15 (the version
//     the shipped xcframework is built against).
//   • Optionally forces the New Architecture flag on both platforms.
//
// The INTERNET permission that model downloads need is not added here: Expo's
// default prebuild manifest already declares android.permission.INTERNET.
//
// Usage (app.json / app.config.js):
//
//   {
//     "expo": {
//       "plugins": [
//         "react-native-nobodywho"
//       ]
//     }
//   }
//
// With options:
//
//   {
//     "expo": {
//       "plugins": [
//         ["react-native-nobodywho", {
//           "android": { "minSdkVersion": 24 },
//           "ios": { "deploymentTarget": "16.0" },
//           "newArchEnabled": true
//         }]
//       ]
//     }
//   }
//
// After changing plugin options, re-run `npx expo prebuild --clean`.

const {
  withGradleProperties,
  withPodfileProperties,
  createRunOncePlugin,
} = require("@expo/config-plugins");
const {
  applyAndroidGradleProperties,
  applyIosPodfileProperties,
} = require("./plugin/properties");
const pkg = require("./package.json");

/**
 * @typedef {import("./plugin/properties").NobodyWhoPluginProps} NobodyWhoPluginProps
 */

/** @type {import("@expo/config-plugins").ConfigPlugin<NobodyWhoPluginProps | void>} */
const withNobodyWho = (config, props = {}) => {
  config = withGradleProperties(config, (cfg) => {
    cfg.modResults = applyAndroidGradleProperties(cfg.modResults, props);
    return cfg;
  });

  config = withPodfileProperties(config, (cfg) => {
    cfg.modResults = applyIosPodfileProperties(cfg.modResults, props);
    return cfg;
  });

  return config;
};

module.exports = createRunOncePlugin(withNobodyWho, pkg.name, pkg.version);
