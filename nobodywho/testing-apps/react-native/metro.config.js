const path = require('path');
const {getDefaultConfig, mergeConfig} = require('@react-native/metro-config');

// Pointing the app at the in-repo binding (`npm install ../../react-native`)
// links it rather than copying, and Metro will not follow a symlink whose
// target lies outside the project root unless that target is watched.
//
// The binding also carries its own node_modules, so React and React Native are
// pinned to this app's copies — otherwise Metro resolves a second instance of
// each from the binding's tree, which fails at runtime rather than at build
// time. Both settings are harmless when the app uses the published package.
const bindingRoot = path.resolve(__dirname, '../../react-native');

/**
 * Metro configuration
 * https://reactnative.dev/docs/metro
 *
 * @type {import('@react-native/metro-config').MetroConfig}
 */
const config = {
  watchFolders: [bindingRoot],
  resolver: {
    nodeModulesPaths: [path.resolve(__dirname, 'node_modules')],
    extraNodeModules: {
      react: path.resolve(__dirname, 'node_modules/react'),
      'react-native': path.resolve(__dirname, 'node_modules/react-native'),
    },
  },
};

module.exports = mergeConfig(getDefaultConfig(__dirname), config);
