#!/usr/bin/env node
/**
 * verify_indexer.js
 *
 * This script verifies that our bitcoin indexer is producing the correct data.
 * It compares:
 *   - The tip (block height and hash)
 *   - For the last 50 blocks, the block hash and txids (in order)
 *   - For each output of each transaction in these blocks, it checks that if the output is unspent
 *     (as per electrs’ /tx/:txid/outspend/:vout endpoint) then the indexer’s /address/:address endpoint
 *     returns that output (with the correct bitcoin value). Spent outputs should not appear.
 *
 * Usage:
 *   node verify_indexer.js
 *
 * Dependencies:
 *   npm install axios bitcoinjs-lib
 */

const axios = require("axios");
const bitcoin = require("bitcoinjs-lib");

// --- CONFIGURATION ---
// Change these as appropriate:
const INDEXER_BASE_URL = "http://localhost:3030"; // your indexer endpoint
const ELECTRS_BASE_URL = "https://electrs.test.arch.network"; // electrs instance

// --- UTILITY FUNCTIONS ---

// Fetch tip from our indexer. We expect JSON { height: number, hash: string }
async function fetchIndexerTip() {
  const url = `${INDEXER_BASE_URL}/tip`;
  const res = await axios.get(url);
  return res.data;
}

// Fetch tip from electrs. (Electrs splits the tip into two endpoints.)
async function fetchElectrsTip() {
  const heightUrl = `${ELECTRS_BASE_URL}/blocks/tip/height`;
  const hashUrl = `${ELECTRS_BASE_URL}/blocks/tip/hash`;
  const [hRes, hashRes] = await Promise.all([
    axios.get(heightUrl),
    axios.get(hashUrl),
  ]);
  return { height: parseInt(hRes.data, 10), hash: hashRes.data };
}

// Given a block height, fetch the block hash from the indexer.
async function fetchIndexerBlockHashByHeight(height) {
  const url = `${INDEXER_BASE_URL}/block/${height}/hash`;
  const res = await axios.get(url);
  return res.data;
}

// Given a block height, fetch the block hash from electrs.
async function fetchElectrsBlockHashByHeight(height) {
  const url = `${ELECTRS_BASE_URL}/block-height/${height}`;
  const res = await axios.get(url);
  return res.data;
}

// Given a block (by its hash or height), fetch the list of txids from the indexer.
async function fetchIndexerBlockTxids(blockQuery) {
  const url = `${INDEXER_BASE_URL}/block/${blockQuery}/txids`;
  const res = await axios.get(url);
  return res.data; // expected array of txid strings
}

// Given a block (by its hash), fetch the list of txids from electrs.
async function fetchElectrsBlockTxids(blockQuery) {
  const url = `${ELECTRS_BASE_URL}/block/${blockQuery}/txids`;
  const res = await axios.get(url);
  return res.data;
}

// Fetch a transaction’s details from electrs (the JSON includes an “output” array).
async function fetchElectrsTransaction(txid) {
  const url = `${ELECTRS_BASE_URL}/tx/${txid}`;
  const res = await axios.get(url);
  return res.data;
}

// Fetch spending info for a given output via electrs.
// We assume the returned JSON contains a Boolean field "spent".
async function fetchElectrsTxOutspend(txid, vout) {
  const url = `${ELECTRS_BASE_URL}/tx/${txid}/outspend/${vout}`;
  const res = await axios.get(url);
  return res.data;
}

// Fetch address data from our indexer.
// Expected JSON: { value: number, runes: [...], outputs: [ { outpoint: { txid, vout }, value, ... } ] }
async function fetchIndexerAddressData(address) {
  const url = `${INDEXER_BASE_URL}/address/${address}`;
  const res = await axios.get(url);
  return res.data;
}

// --- MAIN VERIFICATION ---
async function main() {
  try {
    console.log("--- TIP VERIFICATION ---");
    console.log("Fetching tip from indexer...");
    const indexerTip = await fetchIndexerTip();
    console.log("Indexer tip:", indexerTip);

    console.log("Fetching tip from electrs...");
    const electrsTip = await fetchElectrsTip();
    console.log("Electrs tip:", electrsTip);

    if (indexerTip.height !== electrsTip.height) {
      throw new Error(
        `Tip height mismatch: indexer=${indexerTip.height} vs electrs=${electrsTip.height}`
      );
    }
    if (indexerTip.hash !== electrsTip.hash) {
      throw new Error(
        `Tip blockhash mismatch: indexer=${indexerTip.hash} vs electrs=${electrsTip.hash}`
      );
    }
    console.log("Tip verification PASSED.");

    // Determine the range: last 50 blocks (if tip height is high enough)
    const tipHeight = indexerTip.height;
    const startHeight = tipHeight - 49;
    console.log(
      `\n--- BLOCK VERIFICATION for blocks ${startHeight} to ${tipHeight} ---`
    );

    // This mapping will accumulate the expected (unspent) outputs by address.
    // Structure: { [address]: [ { txid, vout, value } ] }
    const expectedAddressOutputs = {};

    // Loop through the last 50 blocks.
    for (let height = startHeight; height <= tipHeight; height++) {
      console.log(`\nVerifying block at height ${height}...`);

      // Get the block hash from both indexer and electrs.
      const [indexerBlockHash, electrsBlockHash] = await Promise.all([
        fetchIndexerBlockHashByHeight(height),
        fetchElectrsBlockHashByHeight(height),
      ]);
      if (indexerBlockHash !== electrsBlockHash) {
        throw new Error(
          `Block hash mismatch at height ${height}: indexer=${indexerBlockHash} vs electrs=${electrsBlockHash}`
        );
      }
      console.log(`Block hash OK: ${indexerBlockHash}`);

      // Get txids for this block from both sources.
      const [indexerTxids, electrsTxids] = await Promise.all([
        fetchIndexerBlockTxids(indexerBlockHash),
        fetchElectrsBlockTxids(electrsBlockHash),
      ]);
      if (indexerTxids.length !== electrsTxids.length) {
        throw new Error(
          `TXID count mismatch at height ${height}: indexer has ${indexerTxids.length} vs electrs ${electrsTxids.length}`
        );
      }
      for (let i = 0; i < indexerTxids.length; i++) {
        if (indexerTxids[i] !== electrsTxids[i]) {
          throw new Error(
            `TXID mismatch at block ${indexerBlockHash} index ${i}: indexer=${indexerTxids[i]} vs electrs=${electrsTxids[i]}`
          );
        }
      }
      console.log(
        `TXIDs verified for block ${indexerBlockHash} (${indexerTxids.length} transactions).`
      );

      // For every transaction in this block, use electrs to get details.
      for (const txid of electrsTxids) {
        const tx = await fetchElectrsTransaction(txid);
        // Process each output in the transaction.
        for (let vout = 0; vout < tx.output.length; vout++) {
          const output = tx.output[vout];
          // Decode the output script to a bitcoin address.
          let address;
          try {
            // Assume script_pubkey is a hex string.
            const scriptBuffer = Buffer.from(output.script_pubkey, "hex");
            // Using bitcoinjs-lib’s helper to convert the output script into an address.
            // (Change the network if needed.)
            address = bitcoin.address.fromOutputScript(
              scriptBuffer,
              bitcoin.networks.bitcoin
            );
          } catch (err) {
            console.warn(
              `Could not decode address for txid ${txid} vout ${vout}: ${err.message}`
            );
            continue;
          }

          // Determine whether this output is spent.
          const outspend = await fetchElectrsTxOutspend(txid, vout);
          const isSpent = outspend.spent === true;
          if (!isSpent) {
            // Record this unspent output.
            if (!expectedAddressOutputs[address]) {
              expectedAddressOutputs[address] = [];
            }
            expectedAddressOutputs[address].push({
              txid,
              vout,
              value: output.value,
            });
          }
        } // end for each output
      } // end for each transaction
    } // end for each block

    console.log("\n--- ADDRESS OUTPUTS VERIFICATION ---");
    // Now verify that for each address (from our 50 blocks) the indexer returns the correct unspent outputs.
    for (const address of Object.keys(expectedAddressOutputs)) {
      const expectedOutputs = expectedAddressOutputs[address];
      console.log(
        `Verifying address ${address} (expected ${expectedOutputs.length} unspent outputs)...`
      );
      let addressData;
      try {
        addressData = await fetchIndexerAddressData(address);
      } catch (err) {
        throw new Error(
          `Failed to fetch address data for ${address}: ${err.message}`
        );
      }
      const indexedOutputs = addressData.outputs || [];

      // Check that every expected output appears in the indexer’s data.
      for (const exp of expectedOutputs) {
        const found = indexedOutputs.find(
          (o) =>
            o.outpoint.txid === exp.txid && Number(o.outpoint.vout) === exp.vout
        );
        if (!found) {
          throw new Error(
            `Missing unspent output for address ${address}: ${exp.txid}:${exp.vout}`
          );
        }
        if (found.value !== exp.value) {
          throw new Error(
            `Value mismatch for ${address} output ${exp.txid}:${exp.vout}: expected ${exp.value}, got ${found.value}`
          );
        }
      }
      // Also ensure that the indexer did not include any extra outputs.
      if (indexedOutputs.length !== expectedOutputs.length) {
        throw new Error(
          `Address ${address} returned extra outputs. Expected ${expectedOutputs.length} but got ${indexedOutputs.length}`
        );
      }
      console.log(`Address ${address} verified.`);
    }

    console.log("\nALL VERIFICATIONS PASSED SUCCESSFULLY.");
  } catch (err) {
    console.error("Verification failed:", err.message);
    process.exit(1);
  }
}

main();
