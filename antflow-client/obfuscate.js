const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

const isProd = process.argv.includes('--prod');
const srcDir = path.join(__dirname, 'src-tauri', 'frontend');
const dstDir = path.join(__dirname, 'dist');

function copyDir(src, dest) {
  fs.mkdirSync(dest, { recursive: true });
  const entries = fs.readdirSync(src, { withFileTypes: true });

  for (let entry of entries) {
    const srcPath = path.join(src, entry.name);
    const destPath = path.join(dest, entry.name);

    if (entry.isDirectory()) {
      copyDir(srcPath, destPath);
    } else {
      fs.copyFileSync(srcPath, destPath);
    }
  }
}

console.log(`[obfuscate.js] Cleaning and recreating ${dstDir}...`);
if (fs.existsSync(dstDir)) {
  fs.rmSync(dstDir, { recursive: true, force: true });
}
fs.mkdirSync(dstDir, { recursive: true });

console.log(`[obfuscate.js] Copying frontend assets from ${srcDir} to ${dstDir}...`);
try {
  copyDir(srcDir, dstDir);
} catch (err) {
  console.error('[obfuscate.js] Failed to copy frontend directory:', err);
  process.exit(1);
}

if (isProd) {
  console.log('[obfuscate.js] Running javascript-obfuscator on dist/index.js...');
  const targetJs = path.join(dstDir, 'index.js');
  
  // Build command using global javascript-obfuscator
  const cmd = `javascript-obfuscator "${targetJs}" --output "${targetJs}" --control-flow-flattening true --string-array true --string-array-encoding rc4 --self-defending true --debug-protection true --disable-console-output true`;
  
  try {
    execSync(cmd, { stdio: 'inherit' });
    console.log('[obfuscate.js] JS obfuscation completed successfully!');
  } catch (err) {
    console.error('[obfuscate.js] Error during obfuscation:', err);
    process.exit(1);
  }
} else {
  console.log('[obfuscate.js] Dev mode: skipping obfuscation.');
}
