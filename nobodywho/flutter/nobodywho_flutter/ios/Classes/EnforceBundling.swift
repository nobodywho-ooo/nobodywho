import CBinding

// https://github.com/flutter/flutter/pull/96225#issuecomment-1319080539
public func dummyMethodToEnforceBundling() {
    enforce_binding() // disable tree shaking
}
