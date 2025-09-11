// This file is part of the Titan project.
//
// Copyright (c) 2023-2024, Bellscoin (BELL)
//
// This software is provided 'as-is', without any express or implied
// warranty. In no event will the authors be held liable for any damages
// arising from the use of this software.
//
// Permission is granted to anyone to use this software for any purpose,
// including commercial applications, and to alter it and redistribute it
// freely, subject to the following restrictions:
//
// 1. The origin of this software must not be misrepresented; you must not
//    claim that you wrote the original software. If you use this software
//    in a product, an acknowledgment in the product documentation would be
//    appreciated but is not required.
//
// 2. Altered source versions must be plainly marked as such, and must not be
//    misrepresented as being the original software.
//
// 3. This notice may not be removed or altered from any source distribution.
//
// This file is part of the Titan project.
//
// Copyright (c) 2023-2024, Bellscoin (BELL)
//
// This software is provided 'as-is', without any express or implied
// warranty. In no event will the authors be held liable for any damages
// arising from the use of this software.
//
// Permission is granted to anyone to use this software for any purpose,
// including commercial applications, and to alter it and redistribute it
// freely, subject to the following restrictions:
//
// 1. The origin of this software must not be misrepresented; you must not
//    claim that you wrote the original software. If you use this software
//    in a product, an acknowledgment in the product documentation would be
//    appreciated but is not required.
//
// 2. Altered source versions must be plainly marked as such, and must not be
//    misrepresented as being the original software.
//
// 3. This notice may not be removed or altered from any source distribution.

use alkanes_indexer::view;
use anyhow::Result;
use axum::{response::IntoResponse, Json};
use http::StatusCode;
use protobuf::Message;
pub async fn health_check() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(http::header::CONTENT_TYPE, "application/json")],
        Json(serde_json::json!({"status": "ok"})),
    )
        .into_response()
}

pub fn multisumulate(data: &[u8]) -> Result<Vec<u8>> {
    let request =
        alkanes_support::proto::alkanes::MultiSimulateRequest::parse_from_bytes(data).unwrap();
    let mut result: alkanes_support::proto::alkanes::MultiSimulateResponse =
        alkanes_support::proto::alkanes::MultiSimulateResponse::new();
    let responses = view::multi_simulate_safe(&view::parcels_from_protobuf(request), u64::MAX);

    for response in responses {
        let mut res = alkanes_support::proto::alkanes::SimulateResponse::new();
        match response {
            Ok((response, gas_used)) => {
                res.execution = protobuf::MessageField::some(response.into());
                res.gas_used = gas_used;
            }
            Err(e) => {
                result.error = e.to_string();
            }
        }
        result.responses.push(res);
    }

    Ok(result.write_to_bytes().unwrap())
}

pub fn simulate(data: &[u8]) -> Result<Vec<u8>> {
    let request =
        alkanes_support::proto::alkanes::MessageContextParcel::parse_from_bytes(data).unwrap();
    let mut result: alkanes_support::proto::alkanes::SimulateResponse =
        alkanes_support::proto::alkanes::SimulateResponse::new();
    match view::simulate_safe(&view::parcel_from_protobuf(request), u64::MAX) {
        Ok((response, gas_used)) => {
            result.execution = protobuf::MessageField::some(response.into());
            result.gas_used = gas_used;
        }
        Err(e) => {
            result.error = e.to_string();
        }
    }
    Ok(result.write_to_bytes().unwrap())
}

pub fn sequence() -> Result<Vec<u8>> {
    view::sequence()
}

pub fn meta(data: &[u8]) -> Result<Vec<u8>> {
    let request =
        alkanes_support::proto::alkanes::MessageContextParcel::parse_from_bytes(data).unwrap();
    match view::meta_safe(&view::parcel_from_protobuf(request)) {
        Ok(response) => Ok(response),
        Err(_) => Ok(vec![]),
    }
}

pub fn runesbyaddress(data: &[u8]) -> Result<Vec<u8>> {
    let result: protorune_support::proto::protorune::WalletResponse =
        view::protorunes_by_address(&data.to_vec()).unwrap_or_else(|_| {
            protorune_support::proto::protorune::WalletResponse::new()
        });
    Ok(result.write_to_bytes().unwrap())
}

pub fn unwrap(height: u128) -> Result<Vec<u8>> {
    view::unwrap(height)
}

pub fn runesbyoutpoint(data: &[u8]) -> Result<Vec<u8>> {
    let result: protorune_support::proto::protorune::OutpointResponse =
        view::protorunes_by_outpoint(&data.to_vec()).unwrap_or_else(|_| {
            protorune_support::proto::protorune::OutpointResponse::new()
        });
    Ok(result.write_to_bytes().unwrap())
}

pub fn spendablesbyaddress(data: &[u8]) -> Result<Vec<u8>> {
    let result: protorune_support::proto::protorune::WalletResponse =
        view::protorunes_by_address(&data.to_vec()).unwrap_or_else(|_| {
            protorune_support::proto::protorune::WalletResponse::new()
        });
    Ok(result.write_to_bytes().unwrap())
}

pub fn protorunesbyaddress(data: &[u8]) -> Result<Vec<u8>> {
    let mut result: protorune_support::proto::protorune::WalletResponse =
        view::protorunes_by_address(&data.to_vec())
            .unwrap_or_else(|_| protorune_support::proto::protorune::WalletResponse::new());

    result.outpoints = result
        .outpoints
        .into_iter()
        .filter_map(|v| {
            if v.clone()
                .balances
                .unwrap_or_else(|| protorune_support::proto::protorune::BalanceSheet::new())
                .entries
                .len()
                == 0
            {
                None
            } else {
                Some(v)
            }
        })
        .collect::<Vec<protorune_support::proto::protorune::OutpointResponse>>();

    Ok(result.write_to_bytes().unwrap())
}

pub fn getblock(data: &[u8]) -> Result<Vec<u8>> {
    view::getblock(&data.to_vec())
}

pub fn protorunesbyheight(data: &[u8]) -> Result<Vec<u8>> {
    let result: protorune_support::proto::protorune::RunesResponse =
        view::protorunes_by_height(&data.to_vec())
            .unwrap_or_else(|_| protorune_support::proto::protorune::RunesResponse::new());
    Ok(result.write_to_bytes().unwrap())
}

pub fn alkanes_id_to_outpoint(data: &[u8]) -> Result<Vec<u8>> {
    let result: alkanes_support::proto::alkanes::AlkaneIdToOutpointResponse =
        view::alkanes_id_to_outpoint(&data.to_vec()).unwrap_or_else(|err| {
            eprintln!("Error in alkanes_id_to_outpoint: {:?}", err);
            alkanes_support::proto::alkanes::AlkaneIdToOutpointResponse::new()
        });
    Ok(result.write_to_bytes().unwrap())
}

pub fn traceblock(height: u32) -> Result<Vec<u8>> {
    view::traceblock(height)
}

pub fn trace(data: &[u8]) -> Result<Vec<u8>> {
    let outpoint: bitcoin::OutPoint =
        protorune_support::proto::protorune::Outpoint::parse_from_bytes(&data.to_vec())
            .unwrap()
            .try_into()
            .unwrap();
    view::trace(&outpoint)
}

pub fn getbytecode(data: &[u8]) -> Result<Vec<u8>> {
    view::getbytecode(&data.to_vec())
}

pub fn protorunesbyoutpoint(data: &[u8]) -> Result<Vec<u8>> {
    let result: protorune_support::proto::protorune::OutpointResponse =
        view::protorunes_by_outpoint(&data.to_vec())
            .unwrap_or_else(|_| protorune_support::proto::protorune::OutpointResponse::new());

    Ok(result.write_to_bytes().unwrap())
}

pub fn runesbyheight(data: &[u8]) -> Result<Vec<u8>> {
    let result: protorune_support::proto::protorune::RunesResponse =
        view::protorunes_by_height(&data.to_vec())
            .unwrap_or_else(|_| protorune_support::proto::protorune::RunesResponse::new());
    Ok(result.write_to_bytes().unwrap())
}

pub fn getinventory(data: &[u8]) -> Result<Vec<u8>> {
    let result = view::getinventory(
        &alkanes_support::proto::alkanes::AlkaneInventoryRequest::parse_from_bytes(data)
            .unwrap()
            .into(),
    )
    .unwrap();
    Ok(result.write_to_bytes().unwrap())
}

pub fn getstorageat(data: &[u8]) -> Result<Vec<u8>> {
    let result = view::getstorageat(
        &alkanes_support::proto::alkanes::AlkaneStorageRequest::parse_from_bytes(data)
            .unwrap()
            .into(),
    )
    .unwrap();
    Ok(result.write_to_bytes().unwrap())
}