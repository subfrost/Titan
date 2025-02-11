#!/usr/bin/env node
/**
 * verify_indexer.js
 *
 * This script verifies that our bitcoin indexer is producing the correct data.
 * It compares:
 *   - The tip (block height and hash) between the indexer and electrs.
 *   - For the last several blocks, it compares the block hash and the ordered list of txids between the indexer and electrs.
 *   - For every address modified (i.e. that appears in any output in these blocks), it:
 *       (a) Compares basic UTXO fields (txid, vout, value) from the indexer against electrs.
 *       (b) Compares the rune arrays (per output) from the indexer against Maestro.
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
const MEMPOOL_SPACE_BASE_URL = 'https://mempool.space/testnet4/api'; // mempool.space instance

// Maestro configuration – update the URL and API key as needed.
const MAESTRO_BASE_URL = 'https://xbt-testnet.gomaestro-api.org/v0';
const MAESTRO_API_KEY = process.env.MAESTRO_API_KEY;

if (!MAESTRO_API_KEY) {
  throw new Error('MAESTRO_API_KEY is not set');
}

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

// Fetch tip from our indexer. Expected JSON: { height: number, hash: string }
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

// Given a block (by its hash), fetch the list of transactions from electrs.
// (Here we use an internal endpoint that returns full transaction objects.)
async function fetchElectrsBlockTxs(blockQuery) {
  const url = `${ELECTRS_BASE_URL}/internal/block/${blockQuery}/txs`;
  const res = await axios.get(url);
  return res.data;
}

// Fetch a transaction’s details from electrs (expected to include a "vout" array).
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
// Expected JSON: { value: number, runes: [...], outputs: [ { txid, vout, value, runes, ... } ] }
async function fetchIndexerAddressData(address) {
  const url = `${INDEXER_BASE_URL}/address/${address}`;
  const res = await axios.get(url);
  return res.data;
}

// Fetch address UTXO data from electrs.
// Expected to return an array of objects with { txid, vout, value }.
async function fetchElectrsAddressUTXOs(address) {
  const url = `${ELECTRS_BASE_URL}/address/${address}/utxo`;
  const res = await axios.get(url);
  return res.data;
}

// Fetch rune id from our indexer
async function fetchIndexerRuneId(runeId) {
  const url = `${INDEXER_BASE_URL}/rune/${runeId}`;
  const res = await axios.get(url);
  return res.data;
}

async function fetchMempoolSpaceTxOutspend(txid, vout) {
  const url = `${MEMPOOL_SPACE_BASE_URL}/tx/${txid}/outspend/${vout}`;
  const res = await axios.get(url);
  return res.data;
}

// Fetch address UTXO data from Maestro.
// We assume Maestro returns a response matching the following structure:
//
// {
//   data: [ { txid, vout, height, satoshis, script_pubkey, address, mempool, runes, ... }, ... ],
//   indexer_info: { ... },
//   next_cursor: string
// }
async function fetchMaestroUTXOs(address) {
  const url = `${MAESTRO_BASE_URL}/mempool/addresses/${address}/utxos?order=desc`;
  const res = await axios.get(url, {
    headers: {
      'api-key': MAESTRO_API_KEY,
      'Content-Type': 'application/json',
    },
  });
  return res.data.data;
}

// --- MAIN VERIFICATION ---
async function main() {
  try {
    console.log('--- TIP VERIFICATION ---');
    console.log('Fetching tip from indexer...');
    const indexerTip = await fetchIndexerTip();
    console.log('Indexer tip:', indexerTip);

    console.log('Fetching tip from electrs...');
    const electrsTip = await fetchElectrsTip();
    console.log('Electrs tip:', electrsTip);

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

    // Determine the range: last few blocks (adjust as needed).
    const tipHeight = indexerTip.height;
    const startHeight = tipHeight - 10; // using 6 blocks for this example
    console.log(
      `\n--- BLOCK VERIFICATION for blocks ${startHeight} to ${tipHeight} ---`,
    );

    // This mapping will accumulate expected (unspent) outputs by address.
    // Structure: { [address]: [ { txid, vout, value, runes } ] }
    // (We will use the address outputs later for UTXO verification.)
    const expectedAddressOutputs = {};

    // Also keep a set of all addresses encountered ("modified addresses")
    const modifiedAddresses = new Set();

    // Loop through each block.
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
          // (Optionally, you could also record expected outputs here.)
        } // end for each output
      } // end for each transaction
    } // end for each block

    // Convert modifiedAddresses set into an array.
    const addressesModified = Array.from(modifiedAddresses);
    console.log(
      `\nCollected ${addressesModified.length} modified addresses from the last blocks.`,
    );

    console.log('\n--- ADDRESS OUTPUTS VERIFICATION ---');
    // For each modified address, perform two checks:
    //  (a) Compare basic UTXO fields (txid, vout, value) between indexer and electrs.
    //  (b) Compare the rune arrays from indexer outputs against Maestro’s response.
    for (const address of addressesModified) {
      console.log(`Verifying address ${address}...`);
      let indexerData, electrsUTXOs, maestroUTXOs;
      try {
        indexerData = await fetchIndexerAddressData(address);
        electrsUTXOs = await fetchElectrsAddressUTXOs(address);
        maestroUTXOs = await fetchMaestroUTXOs(address);
      } catch (err) {
        console.error(
          `Failed to fetch address data for ${address}: ${err.message}`,
        );
        continue;
      }

      // Normalize indexer outputs (basic fields).
      // We assume indexer outputs are objects with properties: txid, vout, value, runes.
      const normalizedIndexerBasic = (indexerData.outputs || []).map((o) => ({
        txid: o.txid,
        vout: Number(o.vout),
        value: o.value,
      }));

      // Normalize electrs outputs (basic fields).
      const normalizedElectrsOutputs = (electrsUTXOs || []).map((o) => ({
        txid: o.txid,
        vout: Number(o.vout),
        value: o.value,
      }));

      // Sort both arrays by txid and vout.
      const sortFunc = (a, b) => {
        if (a.txid < b.txid) return -1;
        if (a.txid > b.txid) return 1;
        return a.vout - b.vout;
      };
      normalizedIndexerBasic.sort(sortFunc);
      normalizedElectrsOutputs.sort(sortFunc);

      if (normalizedIndexerBasic.length !== normalizedElectrsOutputs.length) {
        if (normalizedIndexerBasic.length > normalizedElectrsOutputs.length) {
          // Find the extra outputs in indexer and print them
          const extraOutputs = normalizedIndexerBasic.filter(
            (o) =>
              !normalizedElectrsOutputs.some(
                (e) => e.txid === o.txid && e.vout === o.vout,
              ),
          );
          console.error(
            `Address ${address} has extra outputs: ${JSON.stringify(extraOutputs)}`,
          );
        } else {
          // Find the missing outputs in electrs and print them
          const missingOutputs = normalizedElectrsOutputs.filter(
            (e) =>
              !normalizedIndexerBasic.some(
                (i) => i.txid === e.txid && i.vout === e.vout,
              ),
          );

          for (const missingOutput of missingOutputs) {
            const mempoolSpaceTxOutspend = await fetchMempoolSpaceTxOutspend(
              missingOutput.txid,
              missingOutput.vout,
            );

            if (mempoolSpaceTxOutspend.spent) {
              console.error(
                `Address ${address} has missing outputs: ${JSON.stringify(
                  missingOutput,
                )}`,
              );
            }
          }
        }
      }

      for (let i = 0; i < normalizedIndexerBasic.length; i++) {
        const idxOut = normalizedIndexerBasic[i];
        const elecOut = normalizedElectrsOutputs.find(
          (o) => o.txid === idxOut.txid && o.vout === idxOut.vout,
        );

        if (!elecOut) {
          try {
            const [mempoolSpaceTxOutspend, electrsTransaction] =
              await Promise.all([
                fetchMempoolSpaceTxOutspend(idxOut.txid, idxOut.vout),
                fetchElectrsTransaction(idxOut.txid),
              ]);

            if (mempoolSpaceTxOutspend.spent) {
              console.error(
                `Address ${address} outpoint is not in electrs: ${JSON.stringify(idxOut)}`,
              );
            }
          } catch (err) {
            console.error(
              `Address ${address} outpoint is not in electrs: ${JSON.stringify(idxOut)}`,
            );
          }

          continue;
        }

        if (
          idxOut.txid !== elecOut.txid ||
          idxOut.vout !== elecOut.vout ||
          idxOut.value !== elecOut.value
        ) {
          throw new Error(
            `Address ${address} basic output mismatch at index ${i}: indexer=${JSON.stringify(
              idxOut,
            )} vs electrs=${JSON.stringify(elecOut)}`,
          );
        }
      }

      // Now check runes arrays using Maestro.
      // Normalize Maestro outputs.
      // Maestro returns each utxo with properties: txid, vout, satoshis (value), runes.
      const normalizedMaestroOutputs = (maestroUTXOs || []).map((o) => ({
        txid: o.txid,
        vout: Number(o.vout),
        value: o.satoshis,
        runes: o.runes, // e.g., [ { rune_id: '...', amount: number }, ... ]
      }));

      // Sort indexer outputs and Maestro outputs by txid and vout.
      const normalizedIndexerForRunes = (indexerData.outputs || []).map(
        (o) => ({
          txid: o.txid,
          vout: Number(o.vout),
          runes: o.runes, // e.g., [ { rune_id: '...', amount: '123' }, ... ]
        }),
      );

      // Helper to normalize the runes array by converting rune amounts to numbers and sorting by rune_id.
      const normalizeRunes = (runes) =>
        (runes || [])
          .map((r) => ({ rune_id: r.rune_id, amount: r.amount }))
          .sort((a, b) => a.rune_id.localeCompare(b.rune_id));

      const uniqueRuneIds = new Set();
      for (const out of normalizedIndexerForRunes) {
        for (const rune of out.runes) {
          uniqueRuneIds.add(rune.rune_id);
        }
      }

      let promises = [];
      for (const runeId of uniqueRuneIds) {
        promises.push(fetchIndexerRuneId(runeId));
      }

      const runeIds = await Promise.all(promises);

      for (let i = 0; i < normalizedIndexerForRunes.length; i++) {
        const idxOut = normalizedIndexerForRunes[i];
        const maestroOut = normalizedMaestroOutputs.find(
          (o) => o.txid === idxOut.txid && o.vout === idxOut.vout,
        );

        if (!maestroOut) {
          // We are sure indexer has the right amount of outputs because we have tested it against electrs. So
          // we can skip this output. Maestro only returns 100 outputs per address.
          continue;
        }

        const idxRunes = normalizeRunes(idxOut.runes);
        const maestroRunes = normalizeRunes(maestroOut.runes);
        if (idxRunes.length !== maestroRunes.length) {
          console.error(
            `Maestro runes: ${JSON.stringify(maestroRunes)}`,
            `Indexer runes: ${JSON.stringify(idxRunes)}`,
          );

          console.error(
            `Address ${address} output ${idxOut.txid}:${idxOut.vout} runes length mismatch: indexer has ${idxRunes.length} vs Maestro has ${maestroRunes.length}`,
          );

          continue;
        }

        for (let j = 0; j < idxRunes.length; j++) {
          const idxRune = idxRunes[j];
          const runeEntry = runeIds.find((r) => r.id === idxRune.rune_id);
          if (!runeEntry) {
            throw new Error(`Rune ${idxRune.rune_id} not found`);
          }

          const maestroRune = maestroRunes.find(
            (r) => r.rune_id === idxRune.rune_id,
          );

          if (!maestroRune) {
            throw new Error(`Rune ${idxRune.rune_id} not found in Maestro`);
          }

          const maestroAmount = maestroRune.amount.toString();
          const indexerAmount = toDecimals(idxRune.amount, runeEntry);

          if (
            Number(indexerAmount) !== Number(maestroAmount) ||
            idxRune.rune_id !== maestroRune.rune_id
          ) {
            throw new Error(
              `Address ${address} output ${idxOut.txid}:${idxOut.vout} runes mismatch at index ${j}: indexer=${JSON.stringify(
                idxRunes[j],
              )} vs Maestro=${JSON.stringify(maestroRune)}. Amounts: indexer=${indexerAmount} vs Maestro=${maestroAmount}`,
            );
          }
        }
      }
    }

    console.log('\nALL VERIFICATIONS PASSED SUCCESSFULLY.');
  } catch (err) {
    console.error('Verification failed:', err.message);
    process.exit(1);
  }
}

function toDecimals(amount, rune_entry) {
  const divisibility = rune_entry.divisibility;
  const cutoff = BigInt(10) ** BigInt(divisibility); // Use BigInt to avoid precision loss

  const whole = BigInt(amount) / cutoff;
  let fractional = BigInt(amount) % cutoff;

  if (fractional === BigInt(0)) {
    return whole.toString();
  } else {
    let width = divisibility;
    while (fractional % BigInt(10) === BigInt(0) && width > 0) {
      fractional /= BigInt(10);
      width--;
    }
    return `${whole}.${fractional.toString().padStart(width, '0')}`;
  }
}

main();
