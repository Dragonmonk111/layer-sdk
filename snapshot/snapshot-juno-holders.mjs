#!/usr/bin/env node
/**
 * JUNO-1 Snapshot Script — JunoClaw Airdrop
 * 
 * Captures all staked JUNO delegations at a target block height:
 * - Staked delegations (via staking/v1beta1/delegations pagination)
 * - Includes ALL validators (active and inactive set)
 * - Applies 250K JUNO hard cap per delegator address
 * - 1:1 airdrop ratio (1 ujclaw per 1 ujuno staked, up to cap)
 * 
 * Usage: node snapshot-juno-holders.mjs --lcd=https://juno-api.polkachu.com --output=snapshot.json --target-block=41655555 --cap=250000000000
 * 
 * If --target-block is set, script waits until that block is reached before snapshotting.
 * Cap is in ujuno (250000 JUNO = 250000000000 ujuno).
 */

import https from 'https';

const args = process.argv.slice(2);
const lcdBase = args.find(a => a.startsWith('--lcd='))?.split('=')[1] || 'https://juno-api.polkachu.com';
const outputFile = args.find(a => a.startsWith('--output='))?.split('=')[1] || 'juno-1-holders-snapshot.json';
const targetBlock = args.find(a => a.startsWith('--target-block=')) ? parseInt(args.find(a => a.startsWith('--target-block=')).split('=')[1]) : null;
const capUjuno = args.find(a => a.startsWith('--cap=')) ? BigInt(args.find(a => a.startsWith('--cap=')).split('=')[1]) : BigInt('250000000000'); // 250K JUNO in ujuno

async function fetchJson(url, retries = 3) {
  for (let attempt = 0; attempt < retries; attempt++) {
    try {
      const result = await new Promise((resolve, reject) => {
        https.get(url, { headers: { 'Accept': 'application/json' } }, (res) => {
          let data = '';
          res.on('data', chunk => data += chunk);
          res.on('end', () => {
            if (res.statusCode === 429) {
              reject(new Error('Rate limited (429)'));
              return;
            }
            if (res.statusCode !== 200) {
              reject(new Error(`HTTP ${res.statusCode}`));
              return;
            }
            try { resolve(JSON.parse(data)); }
            catch (e) { reject(new Error(`Failed to parse JSON from ${url}: ${e.message}`)); }
          });
        }).on('error', reject);
      });
      return result;
    } catch (e) {
      if (attempt < retries - 1) {
        const delay = 1000 * (attempt + 1);
        await new Promise(r => setTimeout(r, delay));
      } else {
        throw e;
      }
    }
  }
}

async function getCurrentBlock() {
  const data = await fetchJson(`${lcdBase}/cosmos/base/tendermint/v1beta1/blocks/latest`);
  return {
    height: parseInt(data.block.header.height),
    time: data.block.header.time,
    chainId: data.block.header.chain_id
  };
}

async function getStakingPool() {
  const data = await fetchJson(`${lcdBase}/cosmos/staking/v1beta1/pool`);
  return data.pool;
}

async function waitForTargetBlock(target) {
  if (!target) return null;
  
  console.log(`Waiting for block ${target}...`);
  let currentHeight = 0;
  
  while (true) {
    try {
      const block = await getCurrentBlock();
      currentHeight = block.height;
      
      if (currentHeight >= target) {
        console.log(`\nTarget block ${target} reached! Current: ${currentHeight}`);
        return block;
      }
      
      const remaining = target - currentHeight;
      const etaMin = Math.ceil((remaining * 2.8) / 60);
      process.stdout.write(`\rBlock ${currentHeight} | ${remaining} blocks remaining | ~${etaMin} min ETA   `);
      
      // Poll every 30 seconds
      await new Promise(resolve => setTimeout(resolve, 30000));
    } catch (e) {
      process.stdout.write(`\nError checking block height: ${e.message}, retrying...\n`);
      await new Promise(resolve => setTimeout(resolve, 10000));
    }
  }
}

async function getAllValidators() {
  const validators = [];
  let nextKey = null;
  
  do {
    let url = `${lcdBase}/cosmos/staking/v1beta1/validators?pagination.limit=200`;
    if (nextKey) url += `&pagination.key=${encodeURIComponent(nextKey)}`;
    try {
      const data = await fetchJson(url);
      if (data.validators) {
        for (const v of data.validators) {
          validators.push(v.operator_address);
        }
      }
      nextKey = data.pagination?.next_key || null;
    } catch (e) {
      console.error(`\nError fetching validators: ${e.message}`);
      break;
    }
  } while (nextKey);
  
  // Also fetch inactive validators
  nextKey = null;
  do {
    let url = `${lcdBase}/cosmos/staking/v1beta1/validators?status=BOND_STATUS_UNSPECIFIED&pagination.limit=200`;
    if (nextKey) url += `&pagination.key=${encodeURIComponent(nextKey)}`;
    try {
      const data = await fetchJson(url);
      if (data.validators) {
        for (const v of data.validators) {
          if (!validators.includes(v.operator_address)) {
            validators.push(v.operator_address);
          }
        }
      }
      nextKey = data.pagination?.next_key || null;
    } catch (e) {
      break;
    }
  } while (nextKey);
  
  // Fetch unbonding validators too
  nextKey = null;
  do {
    let url = `${lcdBase}/cosmos/staking/v1beta1/validators?status=BOND_STATUS_UNBONDING&pagination.limit=200`;
    if (nextKey) url += `&pagination.key=${encodeURIComponent(nextKey)}`;
    try {
      const data = await fetchJson(url);
      if (data.validators) {
        for (const v of data.validators) {
          if (!validators.includes(v.operator_address)) {
            validators.push(v.operator_address);
          }
        }
      }
      nextKey = data.pagination?.next_key || null;
    } catch (e) {
      break;
    }
  } while (nextKey);
  
  // Fetch bonded validators
  nextKey = null;
  do {
    let url = `${lcdBase}/cosmos/staking/v1beta1/validators?status=BOND_STATUS_BONDED&pagination.limit=200`;
    if (nextKey) url += `&pagination.key=${encodeURIComponent(nextKey)}`;
    try {
      const data = await fetchJson(url);
      if (data.validators) {
        for (const v of data.validators) {
          if (!validators.includes(v.operator_address)) {
            validators.push(v.operator_address);
          }
        }
      }
      nextKey = data.pagination?.next_key || null;
    } catch (e) {
      break;
    }
  } while (nextKey);
  
  // Fetch unbonded validators
  nextKey = null;
  do {
    let url = `${lcdBase}/cosmos/staking/v1beta1/validators?status=BOND_STATUS_UNBONDED&pagination.limit=200`;
    if (nextKey) url += `&pagination.key=${encodeURIComponent(nextKey)}`;
    try {
      const data = await fetchJson(url);
      if (data.validators) {
        for (const v of data.validators) {
          if (!validators.includes(v.operator_address)) {
            validators.push(v.operator_address);
          }
        }
      }
      nextKey = data.pagination?.next_key || null;
    } catch (e) {
      break;
    }
  } while (nextKey);
  
  console.log(`Found ${validators.length} validators (all statuses)`);
  return validators;
}

async function getDelegationsForValidator(valAddr) {
  const delegations = [];
  let nextKey = null;
  let pageCount = 0;
  
  do {
    let url = `${lcdBase}/cosmos/staking/v1beta1/validators/${valAddr}/delegations?pagination.limit=100`;
    if (nextKey) url += `&pagination.key=${encodeURIComponent(nextKey)}`;
    try {
      const data = await fetchJson(url);
      if (data.delegation_responses) {
        for (const d of data.delegation_responses) {
          if (d.balance && d.balance.denom === 'ujuno') {
            delegations.push({
              delegator: d.delegation.delegator_address,
              validator: valAddr,
              amount: d.balance.amount
            });
          }
        }
      }
      nextKey = data.pagination?.next_key || null;
      pageCount++;
      if (pageCount > 1) {
        await new Promise(r => setTimeout(r, 200));
      }
    } catch (e) {
      console.error(`\n  Warning: failed to fetch delegations for ${valAddr}: ${e.message}`);
      break;
    }
  } while (nextKey);
  
  return delegations;
}

async function getAllDelegations() {
  console.log('Fetching all validators...');
  const validators = await getAllValidators();
  
  const allDelegations = [];
  let failedValidators = 0;
  for (let i = 0; i < validators.length; i++) {
    try {
      const dels = await getDelegationsForValidator(validators[i]);
      allDelegations.push(...dels);
      if (dels.length === 0) failedValidators++;
    } catch (e) {
      failedValidators++;
    }
    process.stdout.write(`\rValidator ${i + 1}/${validators.length} (${validators[i].slice(0, 12)}...) | ${allDelegations.length} delegations | ${failedValidators} failed`);
    if ((i + 1) % 10 === 0) {
      await new Promise(r => setTimeout(r, 100));
    }
  }
  
  console.log(`\nTotal delegations: ${allDelegations.length}`);
  console.log(`Validators with 0 delegations or errors: ${failedValidators}`);
  return allDelegations;
}

async function main() {
  console.log(`JUNO-1 Snapshot Script — JunoClaw Airdrop`);
  console.log(`LCD: ${lcdBase}`);
  console.log(`Output: ${outputFile}`);
  console.log(`Cap: 250,000 JUNO (${capUjuno.toString()} ujuno)`);
  if (targetBlock) {
    console.log(`Target block: ${targetBlock}`);
  }
  console.log('---');
  
  // Wait for target block if specified
  let block;
  if (targetBlock) {
    block = await waitForTargetBlock(targetBlock);
  } else {
    block = await getCurrentBlock();
  }
  console.log(`Snapshot block: ${block.height} | Time: ${block.time} | Chain: ${block.chainId}`);
  
  const pool = await getStakingPool();
  console.log(`Staking Pool:`);
  console.log(`  Bonded: ${pool.bonded_tokens} ujuno (${(parseInt(pool.bonded_tokens) / 1e6).toFixed(6)} JUNO)`);
  console.log(`  Unbonded: ${pool.not_bonded_tokens} ujuno (${(parseInt(pool.not_bonded_tokens) / 1e6).toFixed(6)} JUNO)`);
  console.log(`  Total: ${parseInt(pool.bonded_tokens) + parseInt(pool.not_bonded_tokens)} ujuno`);
  
  console.log('\nFetching ALL delegations (including inactive validators)...');
  const delegations = await getAllDelegations();
  
  // Aggregate staked balances per delegator
  const stakedBalances = {};
  for (const d of delegations) {
    if (!stakedBalances[d.delegator]) stakedBalances[d.delegator] = BigInt(0);
    stakedBalances[d.delegator] += BigInt(d.amount);
  }
  
  // Apply 250K cap
  let totalBeforeCap = BigInt(0);
  let totalAfterCap = BigInt(0);
  let cappedAccounts = 0;
  let totalExcess = BigInt(0);
  
  const stakedBalancesList = Object.entries(stakedBalances).map(([addr, amount]) => {
    totalBeforeCap += amount;
    let cappedAmount = amount;
    let excess = BigInt(0);
    
    if (amount > capUjuno) {
      cappedAmount = capUjuno;
      excess = amount - capUjuno;
      totalExcess += excess;
      cappedAccounts++;
    }
    
    totalAfterCap += cappedAmount;
    
    return {
      address: addr,
      staked_ujuno: amount.toString(),
      airdrop_ujclaw: cappedAmount.toString(),
      capped: amount > capUjuno,
      excess_ujuno: excess.toString()
    };
  }).sort((a, b) => parseInt(b.staked_ujuno) - parseInt(a.staked_ujuno));
  
  const snapshot = {
    snapshot_date: new Date().toISOString().split('T')[0],
    snapshot_block_height: block.height,
    snapshot_timestamp: block.time,
    chain_id: block.chainId,
    lcd_endpoint: lcdBase,
    parameters: {
      eligibility: "staked JUNO only (all validators, active and inactive)",
      cap: "250000 JUNO (250000000000 ujuno)",
      ratio: "1:1 (1 ujclaw per 1 ujuno staked, up to cap)",
      excess_handling: "excess goes to community pool"
    },
    staking_pool: pool,
    summary: {
      total_delegations: delegations.length,
      total_staked_accounts: Object.keys(stakedBalances).length,
      total_staked_ujuno: totalBeforeCap.toString(),
      total_airdrop_ujclaw: totalAfterCap.toString(),
      total_excess_ujuno: totalExcess.toString(),
      capped_accounts: cappedAccounts
    },
    staked_balances: stakedBalancesList
  };
  
  const { writeFileSync } = await import('fs');
  writeFileSync(outputFile, JSON.stringify(snapshot, null, 2));
  console.log(`\n--- Snapshot Summary ---`);
  console.log(`Block: ${block.height}`);
  console.log(`Accounts: ${Object.keys(stakedBalances).length}`);
  console.log(`Total staked: ${(Number(totalBeforeCap) / 1e6).toFixed(2)} JUNO`);
  console.log(`Total airdrop: ${(Number(totalAfterCap) / 1e6).toFixed(2)} ujclaw`);
  console.log(`Capped accounts: ${cappedAccounts}`);
  console.log(`Excess to community pool: ${(Number(totalExcess) / 1e6).toFixed(2)} JUNO`);
  console.log(`\nSnapshot written to ${outputFile}`);
}

main().catch(console.error);
