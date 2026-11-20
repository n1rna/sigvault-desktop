//! Read-only PSBT (v0, BIP 174) summary for the signing review: outputs,
//! their addresses, which ones return to the wallet, and the fee.
//!
//! The app crate has no `bitcoin` dependency (only the core does), so this
//! is a small dedicated parser. It never signs or modifies anything; on any
//! parse problem it returns `None` and the page falls back to the raw PSBT.

use base64::Engine;

#[derive(Debug, Clone, PartialEq)]
pub struct OutputSummary {
    /// Address, or `script <hex>` for non-standard / legacy scripts.
    pub address: String,
    pub value: u64,
    /// The output carries BIP 32 derivation info, i.e. it pays back to
    /// the wallet (change).
    pub is_change: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PsbtSummary {
    pub input_count: usize,
    pub outputs: Vec<OutputSummary>,
    /// Sum of input values, when every input carries its UTXO.
    pub input_total: Option<u64>,
    pub fee: Option<u64>,
}

impl PsbtSummary {
    /// Value leaving the wallet (non-change outputs).
    pub fn sending(&self) -> u64 {
        self.outputs
            .iter()
            .filter(|o| !o.is_change)
            .map(|o| o.value)
            .sum()
    }
}

/// Bech32 HRP for a network name from session requirements / descriptor.
pub fn hrp_for(network: Option<&str>, descriptor: Option<&str>) -> &'static str {
    match network {
        Some("bitcoin" | "mainnet") => return "bc",
        Some("regtest") => return "bcrt",
        Some("testnet" | "testnet4" | "signet") => return "tb",
        _ => {}
    }
    match descriptor {
        Some(d) if d.contains("xpub") => "bc",
        _ => "tb",
    }
}

pub fn summarize(psbt_base64: &str, hrp: &str) -> Option<PsbtSummary> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(psbt_base64.trim())
        .ok()?;
    let mut r = Reader { b: &bytes, pos: 0 };
    if r.take(5)? != b"psbt\xff" {
        return None;
    }

    // Global map: the unsigned transaction (key type 0x00).
    let mut tx: Option<Tx> = None;
    for (key, value) in r.map()? {
        if key.first() == Some(&0x00) && key.len() == 1 {
            tx = Some(parse_tx(value)?);
        }
    }
    let tx = tx?;

    // Input maps: witness_utxo (0x01) or non_witness_utxo (0x00).
    let mut input_total: Option<u64> = Some(0);
    for txin in &tx.inputs {
        let mut value = None;
        for (key, val) in r.map()? {
            match key.first() {
                Some(0x01) if key.len() == 1 => {
                    let mut vr = Reader { b: val, pos: 0 };
                    value = Some(vr.u64()?);
                }
                Some(0x00) if key.len() == 1 && value.is_none() => {
                    let prev = parse_tx(val)?;
                    value = prev.outputs.get(txin.vout as usize).map(|o| o.value);
                }
                _ => {}
            }
        }
        input_total = match (input_total, value) {
            (Some(t), Some(v)) => Some(t + v),
            _ => None,
        };
    }

    // Output maps: bip32_derivation (0x02) / tap_bip32_derivation (0x07).
    let mut outputs = Vec::with_capacity(tx.outputs.len());
    for out in &tx.outputs {
        let mut is_change = false;
        for (key, _) in r.map()? {
            if matches!(key.first(), Some(0x02 | 0x07)) {
                is_change = true;
            }
        }
        outputs.push(OutputSummary {
            address: script_to_address(&out.script, hrp),
            value: out.value,
            is_change,
        });
    }

    let out_total: u64 = outputs.iter().map(|o| o.value).sum();
    let fee = input_total.and_then(|i| i.checked_sub(out_total));
    Some(PsbtSummary {
        input_count: tx.inputs.len(),
        outputs,
        input_total,
        fee,
    })
}

// ── transaction parsing ──

struct TxIn {
    vout: u32,
}

struct TxOut {
    value: u64,
    script: Vec<u8>,
}

struct Tx {
    inputs: Vec<TxIn>,
    outputs: Vec<TxOut>,
}

fn parse_tx(b: &[u8]) -> Option<Tx> {
    let mut r = Reader { b, pos: 0 };
    r.take(4)?; // version
    let mut n_in = r.varint()?;
    let mut segwit = false;
    if n_in == 0 {
        // Segwit marker + flag.
        if r.take(1)? != [1] {
            return None;
        }
        segwit = true;
        n_in = r.varint()?;
    }
    let mut inputs = Vec::new();
    for _ in 0..n_in {
        r.take(32)?;
        let vout = u32::from_le_bytes(r.take(4)?.try_into().ok()?);
        let len = r.varint()? as usize;
        r.take(len)?;
        r.take(4)?;
        inputs.push(TxIn { vout });
    }
    let n_out = r.varint()?;
    let mut outputs = Vec::new();
    for _ in 0..n_out {
        let value = r.u64()?;
        let len = r.varint()? as usize;
        outputs.push(TxOut {
            value,
            script: r.take(len)?.to_vec(),
        });
    }
    if segwit {
        for _ in 0..n_in {
            let items = r.varint()?;
            for _ in 0..items {
                let len = r.varint()? as usize;
                r.take(len)?;
            }
        }
    }
    r.take(4)?; // locktime
    Some(Tx { inputs, outputs })
}

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let s = self.b.get(self.pos..end)?;
        self.pos = end;
        Some(s)
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn varint(&mut self) -> Option<u64> {
        let first = self.take(1)?[0];
        Some(match first {
            0xfd => u16::from_le_bytes(self.take(2)?.try_into().ok()?) as u64,
            0xfe => u32::from_le_bytes(self.take(4)?.try_into().ok()?) as u64,
            0xff => u64::from_le_bytes(self.take(8)?.try_into().ok()?),
            n => n as u64,
        })
    }
    /// One PSBT key-value map, up to its 0x00 separator.
    fn map(&mut self) -> Option<Vec<(&'a [u8], &'a [u8])>> {
        let mut out = Vec::new();
        loop {
            let klen = self.varint()? as usize;
            if klen == 0 {
                return Some(out);
            }
            let key = self.take(klen)?;
            let vlen = self.varint()? as usize;
            let val = self.take(vlen)?;
            out.push((key, val));
        }
    }
}

// ── addresses ──

fn script_to_address(script: &[u8], hrp: &str) -> String {
    let segwit = match script {
        [0x00, 0x14, prog @ ..] if prog.len() == 20 => Some((0u8, prog)),
        [0x00, 0x20, prog @ ..] if prog.len() == 32 => Some((0u8, prog)),
        [0x51, 0x20, prog @ ..] if prog.len() == 32 => Some((1u8, prog)),
        _ => None,
    };
    match segwit {
        Some((version, prog)) => bech32_segwit(hrp, version, prog),
        None => {
            let hex: String = script.iter().map(|b| format!("{b:02x}")).collect();
            format!("script {hex}")
        }
    }
}

const CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";

fn polymod(values: &[u8]) -> u32 {
    const GEN: [u32; 5] = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];
    let mut chk: u32 = 1;
    for v in values {
        let top = chk >> 25;
        chk = ((chk & 0x1ffffff) << 5) ^ (*v as u32);
        for (i, g) in GEN.iter().enumerate() {
            if (top >> i) & 1 == 1 {
                chk ^= g;
            }
        }
    }
    chk
}

fn bech32_segwit(hrp: &str, version: u8, program: &[u8]) -> String {
    // 8-bit → 5-bit groups.
    let mut data = vec![version];
    let (mut acc, mut bits) = (0u32, 0u32);
    for b in program {
        acc = (acc << 8) | *b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            data.push(((acc >> bits) & 31) as u8);
        }
    }
    if bits > 0 {
        data.push(((acc << (5 - bits)) & 31) as u8);
    }
    let constant = if version == 0 { 1 } else { 0x2bc830a3 };
    let mut values: Vec<u8> = hrp.bytes().map(|c| c >> 5).collect();
    values.push(0);
    values.extend(hrp.bytes().map(|c| c & 31));
    values.extend(&data);
    values.extend([0u8; 6]);
    let pm = polymod(&values) ^ constant;
    let mut out = String::from(hrp);
    out.push('1');
    for d in &data {
        out.push(CHARSET[*d as usize] as char);
    }
    for i in 0..6 {
        out.push(CHARSET[((pm >> (5 * (5 - i))) & 31) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    use bitcoin::bip32::{DerivationPath, Fingerprint};
    use bitcoin::{
        absolute, transaction, Address, Amount, Network, OutPoint, Psbt, ScriptBuf, Sequence,
        Transaction, TxIn, TxOut, Witness,
    };

    #[test]
    fn summarizes_outputs_change_and_fee() {
        let dest = Address::from_str("tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx")
            .unwrap()
            .require_network(Network::Testnet)
            .unwrap();
        let change =
            Address::from_str("tb1pqqqqp399et2xygdj5xreqhjjvcmzhxw4aywxecjdzew6hylgvsesf3hn0c")
                .unwrap()
                .require_network(Network::Testnet)
                .unwrap();
        let tx = Transaction {
            version: transaction::Version::TWO,
            lock_time: absolute::LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint::null(),
                script_sig: ScriptBuf::new(),
                sequence: Sequence::MAX,
                witness: Witness::new(),
            }],
            output: vec![
                TxOut {
                    value: Amount::from_sat(40_000),
                    script_pubkey: dest.script_pubkey(),
                },
                TxOut {
                    value: Amount::from_sat(59_000),
                    script_pubkey: change.script_pubkey(),
                },
            ],
        };
        let mut psbt = Psbt::from_unsigned_tx(tx).unwrap();
        psbt.inputs[0].witness_utxo = Some(TxOut {
            value: Amount::from_sat(100_000),
            script_pubkey: dest.script_pubkey(),
        });
        let pk = bitcoin::secp256k1::PublicKey::from_str(
            "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
        )
        .unwrap();
        psbt.outputs[1].bip32_derivation.insert(
            pk,
            (
                Fingerprint::from([1, 2, 3, 4]),
                DerivationPath::from_str("m/1/0").unwrap(),
            ),
        );

        let s = summarize(
            &base64::engine::general_purpose::STANDARD.encode(psbt.serialize()),
            "tb",
        )
        .expect("parses");
        assert_eq!(s.input_count, 1);
        assert_eq!(s.outputs.len(), 2);
        assert_eq!(s.outputs[0].address, dest.to_string());
        assert!(!s.outputs[0].is_change);
        assert_eq!(s.outputs[1].address, change.to_string());
        assert!(s.outputs[1].is_change);
        assert_eq!(s.fee, Some(1_000));
        assert_eq!(s.sending(), 40_000);
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(summarize("not a psbt", "tb"), None);
    }
}
