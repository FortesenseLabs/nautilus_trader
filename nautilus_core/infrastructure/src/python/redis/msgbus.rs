// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2024 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
//  You may not use this file except in compliance with the License.
//  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.
// -------------------------------------------------------------------------------------------------

use std::{collections::HashMap, sync::Arc};

use bytes::Bytes;
use futures::{pin_mut, stream::StreamExt};
use nautilus_common::msgbus::database::MessageBusDatabaseAdapter;
use nautilus_core::{
    python::{to_pyruntime_err, to_pyvalue_err},
    uuid::UUID4,
};
use nautilus_model::identifiers::TraderId;
use pyo3::{prelude::*, types::PyBytes};
use tracing::error;

use crate::redis::msgbus::{BusMessage, RedisMessageBusDatabase};

#[pymethods]
impl RedisMessageBusDatabase {
    #[new]
    fn py_new(trader_id: TraderId, instance_id: UUID4, config_json: Vec<u8>) -> PyResult<Self> {
        let config: HashMap<String, serde_json::Value> =
            serde_json::from_slice(&config_json).map_err(to_pyvalue_err)?;

        match Self::new(trader_id, instance_id, config) {
            Ok(cache) => Ok(cache),
            Err(e) => Err(to_pyruntime_err(e.to_string())),
        }
    }

    #[pyo3(name = "publish")]
    fn py_publish(&self, topic: String, payload: Vec<u8>) -> PyResult<()> {
        self.publish(
            Bytes::copy_from_slice(topic.as_bytes()),
            Bytes::from(payload),
        )
        .map_err(to_pyruntime_err)
    }

    #[pyo3(name = "stream")]
    fn py_stream<'py>(
        &mut self,
        callback: PyObject,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let stream_rx = self.get_stream_receiver().map_err(to_pyruntime_err)?;
        let stream = RedisMessageBusDatabase::stream(stream_rx);
        pyo3_asyncio_0_21::tokio::future_into_py(py, async move {
            pin_mut!(stream);
            while let Some(msg) = stream.next().await {
                Python::with_gil(|py| {
                    let data = PyBytes::new_bound(py, msg.payload.as_ref()).into_py(py);
                    call_python(py, &callback, data);
                })
            }
            Ok(())
        })
    }

    #[pyo3(name = "close")]
    fn py_close(&mut self) -> PyResult<()> {
        self.close().map_err(to_pyruntime_err)
    }
}

fn call_python(py: Python, callback: &PyObject, py_obj: PyObject) -> PyResult<()> {
    callback.call1(py, (py_obj,)).map_err(|e| {
        error!("Error calling Python: {e}");
        e
    })?;
    Ok(())
}
