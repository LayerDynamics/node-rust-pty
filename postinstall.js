const { execSync } = require('child_process');
const os = require('os');
const path = require('path');
const fs = require('fs');
const https = require('https');

const platform = os.platform();
const arch = os.arch();

const binaryBaseURL = 'https://yourdomain.com/prebuilds'; // Replace with your binary hosting URL
const downloadDir = path.resolve(__dirname, 'prebuilds');

const binaries = {
	'darwin-x64': `${binaryBaseURL}/darwin-x64/mybinary.node`,
	'linux-x64': `${binaryBaseURL}/linux-x64/mybinary.node`,
	'win32-x64': `${binaryBaseURL}/win32-x64/mybinary.node`,
};

const key = `${platform}-${arch}`;
const binaryUrl = binaries[key];

if (!binaryUrl) {
	console.error(`No binary available for ${platform}-${arch}`);
	process.exit(1);
}

const targetPath = path.join(downloadDir, `${key}.node`);

// Create download dir if not exists
if (!fs.existsSync(downloadDir)) {
	fs.mkdirSync(downloadDir, { recursive: true });
}

// Function to download the binary
function download(url, dest, cb) {
	const file = fs.createWriteStream(dest);
	https
		.get(url, (response) => {
			response.pipe(file);
			file.on('finish', () => {
				file.close(cb);
			});
		})
		.on('error', (err) => {
			fs.unlink(dest);
			console.error(`Error downloading binary from ${url}`, err);
			process.exit(1);
		});
}

// Download the correct binary
download(binaryUrl, targetPath, () => {
	console.log(`Binary downloaded successfully to ${targetPath}`);
});
