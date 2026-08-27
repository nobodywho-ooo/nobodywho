// Tests for the Expo config plugin's pure value-resolution helpers.
// These exercise the logic in `plugin/properties.js` without pulling in the
// `@expo/config-plugins` mod machinery.

// eslint-disable-next-line @typescript-eslint/no-var-requires
const properties = require("../plugin/properties");

const {
  DEFAULT_ANDROID_MIN_SDK,
  IOS_MIN_DEPLOYMENT_TARGET,
  resolveMinSdkVersion,
  resolveIosDeploymentTarget,
  formatIosDeploymentTarget,
  applyAndroidGradleProperties,
  applyIosPodfileProperties,
  getGradleProperty,
} = properties;

describe("resolveMinSdkVersion", () => {
  test("defaults to 31 when nothing is set", () => {
    expect(resolveMinSdkVersion(undefined, undefined)).toBe(31);
    expect(DEFAULT_ANDROID_MIN_SDK).toBe(31);
  });

  test("honors an explicit lower override", () => {
    expect(resolveMinSdkVersion(undefined, 24)).toBe(24);
  });

  test("honors an explicit higher override", () => {
    expect(resolveMinSdkVersion(undefined, 34)).toBe(34);
  });

  test("never lowers an existing higher value", () => {
    // Existing 34, default requested 31 -> keep 34.
    expect(resolveMinSdkVersion("34", undefined)).toBe(34);
    // Existing 34, explicit 24 -> still keep 34.
    expect(resolveMinSdkVersion("34", 24)).toBe(34);
  });

  test("raises an existing lower value to the default", () => {
    expect(resolveMinSdkVersion("24", undefined)).toBe(31);
  });

  test("ignores an unparseable existing value", () => {
    expect(resolveMinSdkVersion("not-a-number", undefined)).toBe(31);
  });
});

describe("resolveIosDeploymentTarget", () => {
  test("leaves a compliant Expo default untouched", () => {
    // Expo ships 15.1; no prop -> no change.
    expect(resolveIosDeploymentTarget("15.1", undefined)).toBeNull();
    // Nothing recorded yet -> nothing to do (Expo's own default applies).
    expect(resolveIosDeploymentTarget(undefined, undefined)).toBeNull();
  });

  test("raises an existing target that is below the library minimum", () => {
    expect(resolveIosDeploymentTarget("13.0", undefined)).toBe("15.0");
    expect(IOS_MIN_DEPLOYMENT_TARGET).toBe(15.0);
  });

  test("applies an explicit override at or above the minimum", () => {
    expect(resolveIosDeploymentTarget("15.1", "16.0")).toBe("16.0");
    expect(resolveIosDeploymentTarget(undefined, 16)).toBe("16.0");
  });

  test("floors an explicit override below the minimum", () => {
    expect(resolveIosDeploymentTarget(undefined, "13.0")).toBe("15.0");
  });

  test("never lowers an existing higher target via override", () => {
    expect(resolveIosDeploymentTarget("17.0", "16.0")).toBe("17.0");
  });
});

describe("formatIosDeploymentTarget", () => {
  test("keeps integers as x.0 strings", () => {
    expect(formatIosDeploymentTarget(15)).toBe("15.0");
    expect(formatIosDeploymentTarget(16)).toBe("16.0");
  });

  test("preserves fractional targets", () => {
    expect(formatIosDeploymentTarget(16.4)).toBe("16.4");
  });
});

describe("applyAndroidGradleProperties", () => {
  test("adds android.minSdkVersion when absent (default 31)", () => {
    const props = applyAndroidGradleProperties([], {});
    expect(getGradleProperty(props, "android.minSdkVersion")).toBe("31");
  });

  test("updates an existing lower android.minSdkVersion", () => {
    const props = applyAndroidGradleProperties(
      [{ type: "property", key: "android.minSdkVersion", value: "24" }],
      {},
    );
    expect(getGradleProperty(props, "android.minSdkVersion")).toBe("31");
  });

  test("respects an explicit override", () => {
    const props = applyAndroidGradleProperties([], {
      android: { minSdkVersion: 24 },
    });
    expect(getGradleProperty(props, "android.minSdkVersion")).toBe("24");
  });

  test("only writes newArchEnabled when explicitly provided", () => {
    const untouched = applyAndroidGradleProperties([], {});
    expect(getGradleProperty(untouched, "newArchEnabled")).toBeUndefined();

    const enabled = applyAndroidGradleProperties([], { newArchEnabled: true });
    expect(getGradleProperty(enabled, "newArchEnabled")).toBe("true");

    const disabled = applyAndroidGradleProperties([], { newArchEnabled: false });
    expect(getGradleProperty(disabled, "newArchEnabled")).toBe("false");
  });

  test("does not create duplicate entries on re-apply", () => {
    let props: any[] = [];
    props = applyAndroidGradleProperties(props, {});
    props = applyAndroidGradleProperties(props, {});
    const matches = props.filter(
      (p) => p.type === "property" && p.key === "android.minSdkVersion",
    );
    expect(matches).toHaveLength(1);
  });
});

describe("applyIosPodfileProperties", () => {
  test("leaves a compliant default untouched", () => {
    const mod = applyIosPodfileProperties({ "ios.deploymentTarget": "15.1" }, {});
    expect(mod["ios.deploymentTarget"]).toBe("15.1");
  });

  test("raises a sub-minimum target", () => {
    const mod = applyIosPodfileProperties({ "ios.deploymentTarget": "13.0" }, {});
    expect(mod["ios.deploymentTarget"]).toBe("15.0");
  });

  test("applies an explicit override", () => {
    const mod = applyIosPodfileProperties({}, { ios: { deploymentTarget: "16.0" } });
    expect(mod["ios.deploymentTarget"]).toBe("16.0");
  });

  test("only writes newArchEnabled when explicitly provided", () => {
    const untouched = applyIosPodfileProperties({}, {});
    expect(untouched.newArchEnabled).toBeUndefined();

    const enabled = applyIosPodfileProperties({}, { newArchEnabled: true });
    expect(enabled.newArchEnabled).toBe("true");
  });
});
