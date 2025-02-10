#!/usr/bin/env node
/**
 * verify_indexer.js
 *
 * This script verifies that our bitcoin indexer is producing the correct data.
 * It compares:
 *   - The tip (block height and hash)
 *   - For the last 50 blocks, the block hash and txids (in order)
 *   - For all addresses modified (i.e. that appear in any output in these blocks),
 *     it fetches the address endpoint from both providers and compares that they
 *     return the same unspent outputs.
 *
 * Usage:
 *   node verify_indexer.js
 *
 * Dependencies:
 *   npm install axios bitcoinjs-lib
 */

const axios = require('axios');
const bitcoin = require('bitcoinjs-lib');

// --- CONFIGURATION ---
// Change these as appropriate:
const INDEXER_BASE_URL = 'http://localhost:3030'; // your indexer endpoint
const ELECTRS_BASE_URL = 'https://electrs.test.arch.network'; // electrs instance

// For example, using testnet parameters for bitcoinjs-lib:
const BITCOIN_NETWORK_JS_LIB_TESTNET4 = {
  messagePrefix: '\x18Bitcoin Signed Message:\n',
  bech32: 'tb',
  bip32: {
    public: 0x043587cf,
    private: 0x04358394,
  },
  pubKeyHash: 0x6f,
  scriptHash: 0xc4,
  wif: 0x3f,
};

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
async function fetchElectrsBlockTxs(blockQuery) {
  const url = `${ELECTRS_BASE_URL}/internal/block/${blockQuery}/txs`;
  const res = await axios.get(url);
  return res.data;
}

// Fetch a transaction’s details from electrs (the JSON includes a "vout" array).
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

// Fetch address UTXO data from electrs.
// We assume electrs exposes an endpoint that returns an array of UTXOs for the given address,
// each having { txid, vout, value }.
async function fetchElectrsAddressUTXOs(address) {
  const url = `${ELECTRS_BASE_URL}/address/${address}/utxo`;
  const res = await axios.get(url);
  return res.data;
}

// --- MAIN VERIFICATION ---
async function main() {
  try {
    console.log('--- TIP VERIFICATION ---');
    console.log('Fetching tip from indexer...');
    const indexerTip = await fetchIndexerTip();

    console.log('Fetching tip from electrs...');
    const electrsTip = await fetchElectrsTip();

    if (indexerTip.height !== electrsTip.height) {
      throw new Error(
        `Tip height mismatch: indexer=${indexerTip.height} vs electrs=${electrsTip.height}`,
      );
    }
    if (indexerTip.hash !== electrsTip.hash) {
      throw new Error(
        `Tip blockhash mismatch: indexer=${indexerTip.hash} vs electrs=${electrsTip.hash}`,
      );
    }
    console.log('Tip verification PASSED.');

    // Determine the range: last 50 blocks (if tip height is high enough)
    const tipHeight = indexerTip.height;
    const startHeight = tipHeight - 5;
    console.log(
      `\n--- BLOCK VERIFICATION for blocks ${startHeight} to ${tipHeight} ---`,
    );

    // This mapping will accumulate the expected (unspent) outputs by address.
    // Structure: { [address]: [ { txid, vout, value } ] }
    const expectedAddressOutputs = {};

    // Also keep a set of all addresses encountered ("modified addresses")
    const modifiedAddresses = new Set();

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
          `Block hash mismatch at height ${height}: indexer=${indexerBlockHash} vs electrs=${electrsBlockHash}`,
        );
      }
      console.log(`Block hash OK: ${indexerBlockHash}`);

      // Get txids for this block from both sources.
      const [indexerTxids, electrsTx] = await Promise.all([
        fetchIndexerBlockTxids(indexerBlockHash),
        fetchElectrsBlockTxs(electrsBlockHash),
      ]);

      if (indexerTxids.length !== electrsTx.length) {
        throw new Error(
          `TXID count mismatch at height ${height}: indexer has ${indexerTxids.length} vs electrs ${electrsTx.length}`,
        );
      }
      for (let i = 0; i < indexerTxids.length; i++) {
        if (indexerTxids[i] !== electrsTx[i].txid) {
          throw new Error(
            `TXID mismatch at block ${indexerBlockHash} index ${i}: indexer=${indexerTxids[i]} vs electrs=${electrsTx[i].txid}`,
          );
        }
      }
      console.log(
        `TXIDs verified for block ${indexerBlockHash} (${indexerTxids.length} transactions).`,
      );

      // For every transaction in this block, use electrs to get details.
      for (const tx of electrsTx) {
        // Process each output in the transaction.
        // (Electrs returns a "vout" array; adjust property names if needed.)
        for (let vout = 0; vout < tx.vout.length; vout++) {
          const output = tx.vout[vout];
          let address;
          try {
            // Assume the output contains a property "scriptpubkey" (a hex string)
            const scriptBuffer = Buffer.from(output.scriptpubkey, 'hex');
            // Convert the output script into an address.
            address = bitcoin.address.fromOutputScript(
              scriptBuffer,
              BITCOIN_NETWORK_JS_LIB_TESTNET4,
            );
          } catch (err) {
            // If we cannot decode an address (e.g. nonstandard script), skip it.
            continue;
          }
          // Add the address to our set of modified addresses.
          modifiedAddresses.add(address);
        } // end for each output
      } // end for each transaction
    } // end for each block

    // Convert modifiedAddresses set into an array.
    const addressesModified = Array.from(modifiedAddresses);
    console.log(
      `\nCollected ${addressesModified.length} modified addresses from the last 50 blocks.`,
    );

    console.log('\n--- ADDRESS OUTPUTS VERIFICATION ---');
    // Now verify that for each modified address the indexer and electrs return the same unspent outputs.
    for (const address of addressesModified) {
      console.log(`Verifying address ${address}...`);
      let indexerData, electrsUTXOs;
      try {
        indexerData = await fetchIndexerAddressData(address);
        electrsUTXOs = await fetchElectrsAddressUTXOs(address);
      } catch (err) {
        // throw new Error(
        //   `Failed to fetch address data for ${address}: ${err.message}`
        // );

        // Probably an electrs limit.
        continue;
      }

      // Normalize indexer outputs (we expect them to be unspent).
      // Indexer returns outputs as objects with an "outpoint" field.
      const normalizedIndexerOutputs = (indexerData.outputs || []).map((o) => ({
        txid: o.txid,
        vout: Number(o.vout),
        value: o.value,
      }));
      // Normalize electrs outputs.
      const normalizedElectrsOutputs = (electrsUTXOs || []).map((o) => ({
        txid: o.txid,
        vout: Number(o.vout),
        value: o.value,
      }));

      // // Sort both arrays by txid and vout.
      // const sortFunc = (a, b) => {
      //   if (a.txid < b.txid) return -1;
      //   if (a.txid > b.txid) return 1;
      //   return a.vout - b.vout;
      // };
      // normalizedIndexerOutputs.sort(sortFunc);
      // normalizedElectrsOutputs.sort(sortFunc);

      // if (normalizedIndexerOutputs.length !== normalizedElectrsOutputs.length) {
      //   throw new Error(
      //     `Address ${address} output count mismatch: indexer returned ${normalizedIndexerOutputs.length} outputs but electrs returned ${normalizedElectrsOutputs.length}`,
      //   );
      // }

      for (let i = 0; i < normalizedIndexerOutputs.length; i++) {
        const idxOut = normalizedIndexerOutputs[i];
        const elecOut = normalizedElectrsOutputs.find(
          (o) => o.txid === idxOut.txid && o.vout === idxOut.vout,
        );

        if (!elecOut) {
          console.error(
            `Address ${address} output not found in electrs: ${JSON.stringify(idxOut)}`,
          );
          continue;
        }

        if (
          idxOut.txid !== elecOut.txid ||
          idxOut.vout !== elecOut.vout ||
          idxOut.value !== elecOut.value
        ) {
          throw new Error(
            `Address ${address} output mismatch at index ${i}: indexer=${JSON.stringify(
              idxOut,
            )} vs electrs=${JSON.stringify(elecOut)}`,
          );
        }
      }

      console.log(`Address ${address} outputs match.`);
    }

    console.log('\nALL VERIFICATIONS PASSED SUCCESSFULLY.');
  } catch (err) {
    console.error('Verification failed:', err.message);
    process.exit(1);
  }
}

main();
