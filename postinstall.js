const os = require('os');
const path = require('path');
const fs = require('fs');
const https = require('https');

const platform = os.platform();
const arch = os.arch();

const packageVersion = require('./package.json').version;
const repoOwner = 'layerdynamics'; // Replace with your GitHub username
const repoName = 'node-rust-pty'; // Replace with your repository name
const tag = `v${packageVersion}`; // Ensure your GitHub release tag matches the package version prefixed with 'v'

const binaryNameMap = {
	'darwin-x64': 'node-rust-pty.darwin-x64.node',
	'darwin-arm64': 'node-rust-pty.darwin-arm64.node',
	'linux-x64': 'node-rust-pty.linux-x64-gnu.node',
	'win32-x64': 'node-rust-pty.win32-x64-msvc.node',
};

const key = `${platform}-${arch}`;
const binaryName = binaryNameMap[key];

if (!binaryName) {
	console.error(`Unsupported platform: ${platform}-${arch}`);
	process.exit(1);
}

const binaryUrl = `https://github.com/${repoOwner}/${repoName}/releases/download/${tag}/${binaryName}`;
const targetPath = path.join(__dirname, binaryName);

// Function to download the binary
function download(url, dest) {
	return new Promise((resolve, reject) => {
		const file = fs.createWriteStream(dest);
		https
			.get(url, (response) => {
				if (response.statusCode !== 200) {
					reject(
						new Error(
							`Failed to download binary. Status code: ${response.statusCode}`,
						),
					);
					return;
				}
				response.pipe(file);
				file.on('finish', () => {
					file.close(resolve);
				});
			})
			.on('error', (err) => {
				fs.unlink(dest, () => {});
				reject(err);
			});
	});
}

// Download the binary
download(binaryUrl, targetPath)
	.then(() => {
		console.log(`Binary downloaded successfully to ${targetPath}`);
	})
	.catch((err) => {
		console.error(`Error downloading binary from ${binaryUrl}`);
		console.error(err);
		process.exit(1);
	});
