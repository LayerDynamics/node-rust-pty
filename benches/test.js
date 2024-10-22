// benches/test.js
const test = require('ava')
const fs = require('fs')
const { exec: originalExec } = require('child_process')
const { runAllBenchmarks } = require('./benchmark')

test.beforeEach((t) => {
  t.context.readdirSync = fs.readdirSync
  t.context.exec = originalExec

  // Mock the fs.readdirSync method
  fs.readdirSync = () => ['benchmark1.js', 'benchmark2.js']

  // Mock the exec method
  t.context.mockExec = (cmd, callback) => {
    if (cmd.includes('benchmark1.js')) {
      callback(null, 'output1', '')
    } else if (cmd.includes('benchmark2.js')) {
      callback(null, 'output2', '')
    } else {
      callback(new Error('Unknown file'), '', 'error')
    }
  }

  global.exec = t.context.mockExec
})

test.afterEach((t) => {
  // Restore the original methods
  fs.readdirSync = t.context.readdirSync
  global.exec = t.context.exec
})

test.serial('runAllBenchmarks should run all benchmarks and summarize results', async (t) => {
  const log = console.log
  const error = console.error
  let logOutput = ''
  let errorOutput = ''

  console.log = (msg) => {
    logOutput += msg + '\n'
  }
  console.error = (msg) => {
    errorOutput += msg + '\n'
  }

  await runAllBenchmarks()

  console.log = log
  console.error = error

  t.true(logOutput.includes('File: benchmark1.js, Time:'))
  t.true(logOutput.includes('File: benchmark2.js, Time:'))
  t.true(logOutput.includes('Benchmark Summary:'))
  t.true(logOutput.includes('Total Files: 2'))
  t.true(logOutput.includes('Total Time:'))
  t.true(logOutput.includes('Average Time:'))
  t.is(errorOutput, '')
})

test.serial('runAllBenchmarks should handle errors gracefully', async (t) => {
  const log = console.log
  const error = console.error
  let logOutput = ''
  let errorOutput = ''

  console.log = (msg) => {
    logOutput += msg + '\n'
  }
  console.error = (msg) => {
    errorOutput += msg + '\n'
  }

  // Modify exec to throw an error for one of the benchmarks
  global.exec = (cmd, callback) => {
    if (cmd.includes('benchmark1.js')) {
      callback(new Error('Test error'), '', 'error')
    } else {
      callback(null, 'output2', '')
    }
  }

  await runAllBenchmarks()

  console.log = log
  console.error = error

  t.true(errorOutput.includes('Error running benchmark for file: benchmark1.js'))
  t.true(errorOutput.includes('Test error'))
  t.true(errorOutput.includes('error'))
  t.true(logOutput.includes('File: benchmark2.js, Time:'))
  t.true(logOutput.includes('Benchmark Summary:'))
  t.true(logOutput.includes('Total Files: 1'))
  t.true(logOutput.includes('Total Time:'))
  t.true(logOutput.includes('Average Time:'))
})
