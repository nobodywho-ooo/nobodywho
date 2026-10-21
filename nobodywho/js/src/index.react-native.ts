/**
 * NobodyWho — React Native entry point, using the JSI bindings.
 *
 * Selected through the `react-native` condition in package.json `exports`.
 * Metro consumes this TypeScript source directly, so it isn't built by tsdown.
 *
 * This file is NOT generated — it is safe to edit.
 */

// Native crate install + bindings initialize. These run as side effects of
// evaluating this module, which the React Native runtime is guaranteed to do
// before any FFI call because this is the package entry point.
//
// The install logic was previously reached only through lazy re-exports from
// the generated `./index.tsx`. In release mode Metro's inlineRequires defers
// lazy re-exports until first use, so consumers whose first interaction was
// Model.load / downloadModel / Chat never triggered `./index.tsx` and FFI
// calls then threw "Cannot read property 'ubrn_…' of undefined". Doing the
// install in the entry point removes that indirection so this can't recur.
import installer from "./NativeNobodywho";
import nobodywho from "./bindings";

installer.installRustCrate();
nobodywho.initialize();

export * from "./wrapper";
