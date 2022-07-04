use anyhow::Result;
use bitcoin::OutPoint;
use bp::seals::txout::CloseMethod;
use rgb::seal;

use crate::{data::structs::BlindResponse, log};

pub fn blind_utxo(utxo: OutPoint) -> Result<(BlindResponse, OutPoint)> {
    let seal = seal::Revealed::new(CloseMethod::TapretFirst, utxo);

    let result = BlindResponse {
        blinding: seal.blinding.to_string(),
        conceal: seal.to_concealed_seal().to_string(),
    };

    log!(format!("blind result: {result:?}"));

    Ok((result, utxo))
}
