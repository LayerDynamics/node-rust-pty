// benches/benchmark.js
const { exec: originalExec } = require('child_process')
const path = require('path')
const fs = require('fs')

const darwinX64Folder = path.join(__dirname, '../darwin-x64')
const files = fs.readdirSync(darwinX64Folder).filter((file) => file.endsWith('.js'))

const runBenchmark = (file) => {
  return new Promise((resolve, reject) => {
    const start = process.hrtime()
    originalExec(`node ${path.join(darwinX64Folder, file)}`, (error, stdout, stderr) => {
      const end = process.hrtime(start)
      const time = end[0] * 1000 + end[1] / 1000000 // convert to milliseconds

      if (error) {
        reject({ file, error, stderr })
      } else {
        resolve({ file, time, stdout })
      }
    })
  })
}

const summarizeResults = (results) => {
  const totalFiles = results.length
  const totalTime = results.reduce((acc, result) => acc + result.time, 0)
  const averageTime = totalTime / totalFiles

  console.log(`\nBenchmark Summary:`)
  console.log(`Total Files: ${totalFiles}`)
  console.log(`Total Time: ${totalTime.toFixed(3)}ms`)
  console.log(`Average Time: ${averageTime.toFixed(3)}ms`)
}

const runAllBenchmarks = async () => {
  const results = []
  for (const file of files) {
    try {
      const result = await runBenchmark(file)
      results.push(result)
      console.log(`File: ${result.file}, Time: ${result.time.toFixed(3)}ms`)
    } catch (error) {
      console.error(`Error running benchmark for file: ${error.file}`)
      console.error(error.error)
      console.error(error.stderr)
    }
  }
  summarizeResults(results)
}

module.exports = { runBenchmark, summarizeResults, runAllBenchmarks, files }
