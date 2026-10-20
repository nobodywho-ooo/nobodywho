/**
 * NobodyWho — React Native bindings for local LLM inference.
 *
 * This is the package entry point. It installs the native crate and
 * re-exports the platform-independent public API from `./wrapper`.
 */

// Native crate install + bindings initialize. These run as side effects of
// evaluating this module, which the React Native runtime is guaranteed to do
// before any FFI call because main.ts is the package main.
//
// The install logic was previously reached only through lazy re-exports from
// ./index. In release mode Metro's inlineRequires defers lazy re-exports
// until first use, so consumers whose first interaction was Model.load /
// downloadModel / Chat never triggered ./index and FFI calls then threw
// "Cannot read property 'ubrn_…' of undefined". Inlining the install here
// removes the ./index indirection so this can't recur.
import installer from "./NativeNobodywho";
import nobodywhoDefault from "./generated";

installer.installRustCrate();
nobodywhoDefault.initialize();

export * from "./wrapper";
