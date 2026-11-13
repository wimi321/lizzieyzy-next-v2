#!/usr/bin/env node
// Verify the existing Tauri/minisign format with Node's built-in Ed25519 primitive.
// No signing key is read by this helper. See minisign-verify 0.3's wire format.
import { createHash, createPublicKey, verify } from 'node:crypto';
import { createReadStream, readFileSync } from 'node:fs';

function decode(text) {
  if (!/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(text)) {
    throw new Error('Invalid base64');
  }
  return Buffer.from(text, 'base64');
}

try {
  const [asset, signatureFile, version] = process.argv.slice(2);
  if (!asset || !signatureFile || !version) throw new Error('Expected asset, signature and version');
  const publicText = decode((process.env.LIZZIE_UPDATER_PUBKEY || '').trim()).toString('utf8');
  const signatureText = decode(readFileSync(signatureFile, 'utf8').trim()).toString('utf8');
  const publicLines = publicText.trimEnd().split('\n');
  const lines = signatureText.trimEnd().split('\n');
  if (publicLines.length !== 2 || lines.length !== 4) throw new Error('Invalid minisign structure');
  const publicBytes = decode(publicLines[1]);
  const signature = decode(lines[1]);
  const globalSignature = decode(lines[3]);
  if (publicBytes.length !== 42 || publicBytes.subarray(0, 2).toString() !== 'Ed'
      || signature.length !== 74 || signature.subarray(0, 2).toString() !== 'ED'
      || globalSignature.length !== 64 || !lines[2].startsWith('trusted comment: ')
      || !publicBytes.subarray(2, 10).equals(signature.subarray(2, 10))) {
    throw new Error('Unsupported signature format or different public key');
  }
  const comment = lines[2].slice('trusted comment: '.length);
  const versions = comment.split('\t').filter((item) => item.startsWith('version:'));
  if (versions.length !== 1 || versions[0] !== `version:${version}`) {
    throw new Error('Signature does not bind the announced version');
  }
  const key = createPublicKey({
    key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), publicBytes.subarray(10)]),
    format: 'der', type: 'spki',
  });
  const hash = createHash('blake2b512');
  for await (const chunk of createReadStream(asset)) hash.update(chunk);
  if (!verify(null, hash.digest(), key, signature.subarray(10))
      || !verify(null, Buffer.concat([signature.subarray(10), Buffer.from(comment)]), key, globalSignature)) {
    throw new Error('Signature verification failed');
  }
} catch (error) {
  console.error(`update signature: FAILED: ${error.message}`);
  process.exitCode = 1;
}
