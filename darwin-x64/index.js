/* tslint:disable */
/* eslint-disable */
/* prettier-ignore */

/* auto-generated by NAPI-RS */

const { existsSync, readFileSync } = require('fs')
const { join } = require('path')

const { platform, arch } = process

let nativeBinding = null
let localFileExisted = false
let loadError = null

function isMusl() {
  // For Node 10
  if (!process.report || typeof process.report.getReport !== 'function') {
    try {
      const lddPath = require('child_process').execSync('which ldd').toString().trim()
      return readFileSync(lddPath, 'utf8').includes('musl')
    } catch (e) {
      return true
    }
  } else {
    const { glibcVersionRuntime } = process.report.getReport().header
    return !glibcVersionRuntime
  }
}

switch (platform) {
  case 'android':
    switch (arch) {
      case 'arm64':
        localFileExisted = existsSync(join(__dirname, 'node-rust-pty.android-arm64.node'))
        try {
          if (localFileExisted) {
            nativeBinding = require('./node-rust-pty.android-arm64.node')
          } else {
            nativeBinding = require('node-rust-pty-android-arm64')
          }
        } catch (e) {
          loadError = e
        }
        break
      case 'arm':
        localFileExisted = existsSync(join(__dirname, 'node-rust-pty.android-arm-eabi.node'))
        try {
          if (localFileExisted) {
            nativeBinding = require('./node-rust-pty.android-arm-eabi.node')
          } else {
            nativeBinding = require('node-rust-pty-android-arm-eabi')
          }
        } catch (e) {
          loadError = e
        }
        break
      default:
        throw new Error(`Unsupported architecture on Android ${arch}`)
    }
    break
  case 'win32':
    switch (arch) {
      case 'x64':
        localFileExisted = existsSync(
          join(__dirname, 'node-rust-pty.win32-x64-msvc.node')
        )
        try {
          if (localFileExisted) {
            nativeBinding = require('./node-rust-pty.win32-x64-msvc.node')
          } else {
            nativeBinding = require('node-rust-pty-win32-x64-msvc')
          }
        } catch (e) {
          loadError = e
        }
        break
      case 'ia32':
        localFileExisted = existsSync(
          join(__dirname, 'node-rust-pty.win32-ia32-msvc.node')
        )
        try {
          if (localFileExisted) {
            nativeBinding = require('./node-rust-pty.win32-ia32-msvc.node')
          } else {
            nativeBinding = require('node-rust-pty-win32-ia32-msvc')
          }
        } catch (e) {
          loadError = e
        }
        break
      case 'arm64':
        localFileExisted = existsSync(
          join(__dirname, 'node-rust-pty.win32-arm64-msvc.node')
        )
        try {
          if (localFileExisted) {
            nativeBinding = require('./node-rust-pty.win32-arm64-msvc.node')
          } else {
            nativeBinding = require('node-rust-pty-win32-arm64-msvc')
          }
        } catch (e) {
          loadError = e
        }
        break
      default:
        throw new Error(`Unsupported architecture on Windows: ${arch}`)
    }
    break
  case 'darwin':
    localFileExisted = existsSync(join(__dirname, 'node-rust-pty.darwin-universal.node'))
    try {
      if (localFileExisted) {
        nativeBinding = require('./node-rust-pty.darwin-universal.node')
      } else {
        nativeBinding = require('node-rust-pty-darwin-universal')
      }
      break
    } catch {}
    switch (arch) {
      case 'x64':
        localFileExisted = existsSync(join(__dirname, 'node-rust-pty.darwin-x64.node'))
        try {
          if (localFileExisted) {
            nativeBinding = require('./node-rust-pty.darwin-x64.node')
          } else {
            nativeBinding = require('node-rust-pty-darwin-x64')
          }
        } catch (e) {
          loadError = e
        }
        break
      case 'arm64':
        localFileExisted = existsSync(
          join(__dirname, 'node-rust-pty.darwin-arm64.node')
        )
        try {
          if (localFileExisted) {
            nativeBinding = require('./node-rust-pty.darwin-arm64.node')
          } else {
            nativeBinding = require('node-rust-pty-darwin-arm64')
          }
        } catch (e) {
          loadError = e
        }
        break
      default:
        throw new Error(`Unsupported architecture on macOS: ${arch}`)
    }
    break
  case 'freebsd':
    if (arch !== 'x64') {
      throw new Error(`Unsupported architecture on FreeBSD: ${arch}`)
    }
    localFileExisted = existsSync(join(__dirname, 'node-rust-pty.freebsd-x64.node'))
    try {
      if (localFileExisted) {
        nativeBinding = require('./node-rust-pty.freebsd-x64.node')
      } else {
        nativeBinding = require('node-rust-pty-freebsd-x64')
      }
    } catch (e) {
      loadError = e
    }
    break
  case 'linux':
    switch (arch) {
      case 'x64':
        if (isMusl()) {
          localFileExisted = existsSync(
            join(__dirname, 'node-rust-pty.linux-x64-musl.node')
          )
          try {
            if (localFileExisted) {
              nativeBinding = require('./node-rust-pty.linux-x64-musl.node')
            } else {
              nativeBinding = require('node-rust-pty-linux-x64-musl')
            }
          } catch (e) {
            loadError = e
          }
        } else {
          localFileExisted = existsSync(
            join(__dirname, 'node-rust-pty.linux-x64-gnu.node')
          )
          try {
            if (localFileExisted) {
              nativeBinding = require('./node-rust-pty.linux-x64-gnu.node')
            } else {
              nativeBinding = require('node-rust-pty-linux-x64-gnu')
            }
          } catch (e) {
            loadError = e
          }
        }
        break
      case 'arm64':
        if (isMusl()) {
          localFileExisted = existsSync(
            join(__dirname, 'node-rust-pty.linux-arm64-musl.node')
          )
          try {
            if (localFileExisted) {
              nativeBinding = require('./node-rust-pty.linux-arm64-musl.node')
            } else {
              nativeBinding = require('node-rust-pty-linux-arm64-musl')
            }
          } catch (e) {
            loadError = e
          }
        } else {
          localFileExisted = existsSync(
            join(__dirname, 'node-rust-pty.linux-arm64-gnu.node')
          )
          try {
            if (localFileExisted) {
              nativeBinding = require('./node-rust-pty.linux-arm64-gnu.node')
            } else {
              nativeBinding = require('node-rust-pty-linux-arm64-gnu')
            }
          } catch (e) {
            loadError = e
          }
        }
        break
      case 'arm':
        if (isMusl()) {
          localFileExisted = existsSync(
            join(__dirname, 'node-rust-pty.linux-arm-musleabihf.node')
          )
          try {
            if (localFileExisted) {
              nativeBinding = require('./node-rust-pty.linux-arm-musleabihf.node')
            } else {
              nativeBinding = require('node-rust-pty-linux-arm-musleabihf')
            }
          } catch (e) {
            loadError = e
          }
        } else {
          localFileExisted = existsSync(
            join(__dirname, 'node-rust-pty.linux-arm-gnueabihf.node')
          )
          try {
            if (localFileExisted) {
              nativeBinding = require('./node-rust-pty.linux-arm-gnueabihf.node')
            } else {
              nativeBinding = require('node-rust-pty-linux-arm-gnueabihf')
            }
          } catch (e) {
            loadError = e
          }
        }
        break
      case 'riscv64':
        if (isMusl()) {
          localFileExisted = existsSync(
            join(__dirname, 'node-rust-pty.linux-riscv64-musl.node')
          )
          try {
            if (localFileExisted) {
              nativeBinding = require('./node-rust-pty.linux-riscv64-musl.node')
            } else {
              nativeBinding = require('node-rust-pty-linux-riscv64-musl')
            }
          } catch (e) {
            loadError = e
          }
        } else {
          localFileExisted = existsSync(
            join(__dirname, 'node-rust-pty.linux-riscv64-gnu.node')
          )
          try {
            if (localFileExisted) {
              nativeBinding = require('./node-rust-pty.linux-riscv64-gnu.node')
            } else {
              nativeBinding = require('node-rust-pty-linux-riscv64-gnu')
            }
          } catch (e) {
            loadError = e
          }
        }
        break
      case 's390x':
        localFileExisted = existsSync(
          join(__dirname, 'node-rust-pty.linux-s390x-gnu.node')
        )
        try {
          if (localFileExisted) {
            nativeBinding = require('./node-rust-pty.linux-s390x-gnu.node')
          } else {
            nativeBinding = require('node-rust-pty-linux-s390x-gnu')
          }
        } catch (e) {
          loadError = e
        }
        break
      default:
        throw new Error(`Unsupported architecture on Linux: ${arch}`)
    }
    break
  default:
    throw new Error(`Unsupported OS: ${platform}, architecture: ${arch}`)
}

if (!nativeBinding) {
  if (loadError) {
    throw loadError
  }
  throw new Error(`Failed to load native binding`)
}

const { getHomeDir, getTempDir, joinPaths, absolutePath, pathExists, isFile, isDir, expandPathNapi, getDefaultShell, getPtyDevicePath, getExpandedPtyDevicePath, Emulator, PtyHandle, PtyMultiplexer, PtyRenderer, handleInput, Renderer, State, style, styledText, styledElement, init, initVirtualDom, PtyWorker, initModule } = nativeBinding

module.exports.getHomeDir = getHomeDir
module.exports.getTempDir = getTempDir
module.exports.joinPaths = joinPaths
module.exports.absolutePath = absolutePath
module.exports.pathExists = pathExists
module.exports.isFile = isFile
module.exports.isDir = isDir
module.exports.expandPathNapi = expandPathNapi
module.exports.getDefaultShell = getDefaultShell
module.exports.getPtyDevicePath = getPtyDevicePath
module.exports.getExpandedPtyDevicePath = getExpandedPtyDevicePath
module.exports.Emulator = Emulator
module.exports.PtyHandle = PtyHandle
module.exports.PtyMultiplexer = PtyMultiplexer
module.exports.PtyRenderer = PtyRenderer
module.exports.handleInput = handleInput
module.exports.Renderer = Renderer
module.exports.State = State
module.exports.style = style
module.exports.styledText = styledText
module.exports.styledElement = styledElement
module.exports.init = init
module.exports.initVirtualDom = initVirtualDom
module.exports.PtyWorker = PtyWorker
module.exports.initModule = initModule
