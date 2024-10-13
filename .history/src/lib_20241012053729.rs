(base) ➜  node-rust-pty napi build --platform darwin-x64 --release 
   Compiling node-rust-pty v0.0.0 (/Users/ryanoboyle/node-rust-pty)
error: 1 positional argument in format string, but no arguments were given
   --> src/lib.rs:476:47
    |
476 |                     info!("Dropping: Slave_fd {} closed successfully.");
    |                                               ^^

error: could not compile `node-rust-pty` (lib) due to 1 previous error
Internal Error: Command failed: cargo build --release
    at genericNodeError (node:internal/errors:984:15)
    at wrappedFn (node:internal/errors:538:14)
    at checkExecSyncError (node:child_process:890:11)
    at Object.execSync (node:child_process:962:15)
    at BuildCommand.<anonymous> (/Users/ryanoboyle/.asdf/installs/nodejs/21.7.2/lib/node_modules/@napi-rs/cli/scripts/index.js:11529:30)
    at Generator.next (<anonymous>)
    at /Users/ryanoboyle/.asdf/installs/nodejs/21.7.2/lib/node_modules/@napi-rs/cli/scripts/index.js:3526:69
    at new Promise (<anonymous>)
    at __awaiter$1 (/Users/ryanoboyle/.asdf/installs/nodejs/21.7.2/lib/node_modules/@napi-rs/cli/scripts/index.js:3522:10)
    at BuildCommand.execute (/Users/ryanoboyle/.asdf/installs/nodejs/21.7.2/lib/node_modules/@napi-rs/cli/scripts/index.js:11299:16)
(base) ➜  node-rust-pty 