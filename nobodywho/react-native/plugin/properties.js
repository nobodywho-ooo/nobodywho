// @ts-check
//
// Pure helpers for the NobodyWho Expo config plugin.
//
// This file intentionally has NO dependency on `@expo/config-plugins` so the
// value-resolution logic can be unit-tested without the Expo mod machinery.
// The actual mods live in `app.plugin.js`, which wires these helpers into
// `withGradleProperties` / `withPodfileProperties`.

/**
 * Android minSdk that NobodyWho defaults to.
 *
 * The x86_64 Android emulator requires minSdk >= 31 because the Rust runtime
 * uses ELF thread-local storage that only became available on that ABI in
 * Android 12. Real arm64 devices work with any minSdk, but the emulator is the
 * common development target, so the plugin raises minSdk to 31 by default.
 * Consumers can override this (including lowering it) via the `android.minSdkVersion`
 * plugin prop.
 */
const DEFAULT_ANDROID_MIN_SDK = 31;

/**
 * Lowest iOS deployment target NobodyWho supports (matches the Swift package,
 * which declares `.iOS(.v15)`). Modern Expo already defaults to 15.1, so the
 * plugin never lowers an existing/default target below this — it only raises a
 * target that is somehow lower, or applies an explicit override.
 */
const IOS_MIN_DEPLOYMENT_TARGET = 15.0;

/**
 * @typedef {Object} NobodyWhoPluginProps
 * @property {{ minSdkVersion?: number }} [android] Android-specific overrides.
 * @property {{ deploymentTarget?: string | number }} [ios] iOS-specific overrides.
 * @property {boolean} [newArchEnabled] When set, forces the New Architecture flag
 *   on both platforms. When omitted, the project's existing value is left untouched.
 */

/**
 * @param {Array<{ type: string, key?: string, value?: string }>} properties
 * @param {string} key
 * @returns {string | undefined}
 */
function getGradleProperty(properties, key) {
  const found = properties.find(
    (item) => item.type === "property" && item.key === key,
  );
  return found ? found.value : undefined;
}

/**
 * Insert or update a gradle property in place.
 *
 * @param {Array<{ type: string, key?: string, value?: string }>} properties
 * @param {string} key
 * @param {string | number | boolean} value
 * @returns {Array<{ type: string, key?: string, value?: string }>}
 */
function upsertGradleProperty(properties, key, value) {
  const strValue = String(value);
  const existing = properties.find(
    (item) => item.type === "property" && item.key === key,
  );
  if (existing) {
    existing.value = strValue;
  } else {
    properties.push({ type: "property", key, value: strValue });
  }
  return properties;
}

/**
 * Compute the minSdk to write, never lowering a value that is already higher.
 *
 * @param {string | number | undefined} existingValue Current gradle property, if any.
 * @param {number | undefined} requestedValue Value from the plugin prop, if any.
 * @returns {number}
 */
function resolveMinSdkVersion(existingValue, requestedValue) {
  const requested =
    requestedValue == null || Number.isNaN(Number(requestedValue))
      ? DEFAULT_ANDROID_MIN_SDK
      : Number(requestedValue);
  const existing = Number(existingValue);
  return Math.max(Number.isNaN(existing) ? 0 : existing, requested);
}

/**
 * Format a numeric deployment target the way Podfile.properties.json expects it
 * (a string such as "15.0" or "16.4").
 *
 * @param {number} value
 * @returns {string}
 */
function formatIosDeploymentTarget(value) {
  return Number.isInteger(value) ? `${value}.0` : String(value);
}

/**
 * Decide what to write for `ios.deploymentTarget`, or `null` to leave it alone.
 *
 * - An explicit prop wins, but is still floored at the library minimum and never
 *   lowers an existing, higher target.
 * - With no prop, only an existing target below the library minimum is raised;
 *   otherwise the project's default (Expo ships 15.1) is left untouched so the
 *   plugin never regresses a newer Expo default.
 *
 * @param {string | number | undefined} existingValue
 * @param {string | number | undefined} requestedValue
 * @returns {string | null}
 */
function resolveIosDeploymentTarget(existingValue, requestedValue) {
  const existing =
    existingValue == null ? undefined : parseFloat(String(existingValue));
  const explicit =
    requestedValue == null ? undefined : parseFloat(String(requestedValue));

  let target;
  if (explicit != null && !Number.isNaN(explicit)) {
    target = Math.max(
      explicit,
      IOS_MIN_DEPLOYMENT_TARGET,
      existing != null && !Number.isNaN(existing) ? existing : 0,
    );
  } else if (
    existing != null &&
    !Number.isNaN(existing) &&
    existing < IOS_MIN_DEPLOYMENT_TARGET
  ) {
    target = IOS_MIN_DEPLOYMENT_TARGET;
  } else {
    return null;
  }

  return formatIosDeploymentTarget(target);
}

/**
 * Apply NobodyWho's Android requirements to a gradle properties array (as
 * exposed by `withGradleProperties`). Mutates and returns the same array.
 *
 * @param {Array<{ type: string, key?: string, value?: string }>} properties
 * @param {NobodyWhoPluginProps} [props]
 * @returns {Array<{ type: string, key?: string, value?: string }>}
 */
function applyAndroidGradleProperties(properties, props = {}) {
  const requested = props.android ? props.android.minSdkVersion : undefined;
  const existing = getGradleProperty(properties, "android.minSdkVersion");
  const minSdk = resolveMinSdkVersion(existing, requested);
  upsertGradleProperty(properties, "android.minSdkVersion", minSdk);

  if (typeof props.newArchEnabled === "boolean") {
    upsertGradleProperty(properties, "newArchEnabled", props.newArchEnabled);
  }

  return properties;
}

/**
 * Apply NobodyWho's iOS requirements to a Podfile properties object (as exposed
 * by `withPodfileProperties`). Mutates and returns the same object.
 *
 * @param {Record<string, string>} modResults
 * @param {NobodyWhoPluginProps} [props]
 * @returns {Record<string, string>}
 */
function applyIosPodfileProperties(modResults, props = {}) {
  const requested = props.ios ? props.ios.deploymentTarget : undefined;
  const target = resolveIosDeploymentTarget(
    modResults["ios.deploymentTarget"],
    requested,
  );
  if (target != null) {
    modResults["ios.deploymentTarget"] = target;
  }

  if (typeof props.newArchEnabled === "boolean") {
    modResults["newArchEnabled"] = String(props.newArchEnabled);
  }

  return modResults;
}

module.exports = {
  DEFAULT_ANDROID_MIN_SDK,
  IOS_MIN_DEPLOYMENT_TARGET,
  getGradleProperty,
  upsertGradleProperty,
  resolveMinSdkVersion,
  resolveIosDeploymentTarget,
  formatIosDeploymentTarget,
  applyAndroidGradleProperties,
  applyIosPodfileProperties,
};
