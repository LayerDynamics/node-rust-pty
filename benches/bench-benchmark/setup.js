// node-rust-pty/benches/setup.js
const fs = require('fs')
const path = require('path')

// Mock the fs.readdirSync method
fs.readdirSync = (dir) => {
  if (dir === path.join(__dirname, '../darwin-x64')) {
    return ['benchmark1.js', 'benchmark2.js']
  }
  return []
}

// Mock the exec method without calling process.exit()
const { exec: originalExec } = require('child_process')
global.exec = (cmd, callback) => {
  if (cmd.includes('benchmark1.js')) {
    callback(null, 'output1', '')
  } else if (cmd.includes('benchmark2.js')) {
    callback(null, 'output2', '')
  } else {
    // Instead of exiting, return an error through the callback
    callback(new Error('Unknown file'), '', 'error')
  }
}
