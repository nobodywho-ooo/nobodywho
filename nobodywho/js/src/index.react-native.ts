/**
 * NobodyWho — React Native entry point, using the JSI bindings.
 *
 * Selected through the `react-native` condition in package.json `exports`.
 * Metro consumes this TypeScript source directly, so it isn't built by tsdown.
 *
 * This file is NOT generated — it is safe to edit.
 */

// These run as side effects of evaluating this module, which React Native
// guarantees happens before any FFI call because this is the package entry
// point. Don't move them into a lazy re-export: Metro's `inlineRequires`
// defers those until first use, and FFI calls would then throw
// "Cannot read property 'ubrn_*' of undefined".
import installer from "./NativeNobodywho";
import nobodywho from "./generated";

// Register the rust crate with Hermes
installer.installRustCrate();
// Initialize the generated bindings: mostly checksums, but also callbacks.
nobodywho.initialize();

export * from "./wrapper";
