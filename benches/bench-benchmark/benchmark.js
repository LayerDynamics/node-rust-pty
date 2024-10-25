/* benches/benchmark.js */

const Benchmark = require('benchmark')
const pathModule = require('../darwin-x64/index.js') // Adjust the path to your N-API module as necessary
const suite = new Benchmark.Suite()

// Helper function to wait for Promises in benchmarks
const asyncBenchmark = (fn) => {
  return {
    defer: true,
    fn: async (deferred) => {
      try {
        await fn()
        deferred.resolve()
      } catch (err) {
        console.error('Error in benchmark execution:', err)
        deferred.resolve() // Resolve even when there's an error
        process.exit(1) // Exit immediately if any benchmark fails
      }
    },
  }
}

// Function Benchmarks
suite.add(
  'getHomeDir',
  asyncBenchmark(async () => {
    await pathModule.getHomeDir()
  })
)

suite.add(
  'getTempDir',
  asyncBenchmark(async () => {
    await pathModule.getTempDir()
  })
)

suite.add(
  'joinPaths',
  asyncBenchmark(async () => {
    await pathModule.joinPaths(['/usr', 'local', 'bin'])
  })
)

// Update absolutePath benchmark to use an existing path
suite.add(
  'absolutePath',
  asyncBenchmark(async () => {
    await pathModule.absolutePath(__dirname) // Use __dirname to ensure it's a valid path
  })
)

suite.add(
  'pathExists',
  asyncBenchmark(async () => {
    await pathModule.pathExists('/usr/bin')
  })
)

suite.add(
  'isFile',
  asyncBenchmark(async () => {
    await pathModule.isFile('/usr/bin/bash')
  })
)

suite.add(
  'isDir',
  asyncBenchmark(async () => {
    await pathModule.isDir('/usr/local')
  })
)

suite.add(
  'expandPathNapi',
  asyncBenchmark(async () => {
    await pathModule.expandPathNapi('~/${USER}/documents')
  })
)

// suite.add(
//   'getDefaultShell',
//   asyncBenchmark(async () => {
//     await pathModule.getDefaultShell()
//   })
// )

suite.add(
  'getPtyDevicePath',
  asyncBenchmark(async () => {
    await pathModule.getPtyDevicePath()
  })
)

suite.add(
  'getExpandedPtyDevicePath',
  asyncBenchmark(async () => {
    await pathModule.getExpandedPtyDevicePath()
  })
)

// // Class Benchmarks
// suite.add(
//   'PtyHandle.new',
//   asyncBenchmark(async () => {
//     await pathModule.PtyHandle.new()
//   })
// )

suite.add(
  'PtyHandle.getPid',
  asyncBenchmark(async () => {
    const ptyHandle = await pathModule.PtyHandle.new()
    await ptyHandle.getPid()
  })
)

suite.add(
  'PtyHandle.write',
  asyncBenchmark(async () => {
    const ptyHandle = await pathModule.PtyHandle.new()
    await ptyHandle.write('Test data')
  })
)

suite.add(
  'PtyHandle.resize',
  asyncBenchmark(async () => {
    const ptyHandle = await pathModule.PtyHandle.new()
    await ptyHandle.resize(120, 40)
  })
)

suite.add(
  'PtyHandle.execute',
  asyncBenchmark(async () => {
    const ptyHandle = await pathModule.PtyHandle.new()
    await ptyHandle.execute('echo "Hello, PTY!"')
  })
)

suite.add(
  'PtyHandle.close',
  asyncBenchmark(async () => {
    const ptyHandle = await pathModule.PtyHandle.new()
    await ptyHandle.close()
  })
)

// Add listeners
suite
  .on('cycle', (event) => {
    console.log(String(event.target))
  })
  .on('error', (event) => {
    console.error(`Error in suite: ${event.target.name} failed with error: ${event.target.error}`)
    process.exit(1) // Exit immediately if any benchmark fails
  })
  .on('complete', function () {
    console.log('Fastest is ' + this.filter('fastest').map('name'))
  })
  // Run async benchmarks
  .run({ async: true })
