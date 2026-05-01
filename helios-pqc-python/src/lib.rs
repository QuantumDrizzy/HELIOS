use pyo3::prelude::*;
use pyo3::exceptions::PyRuntimeError;
use std::net::Shutdown;
use std::os::windows::net::UnixStream; // En Windows 10+
use std::io::{Read, Write};
use helios_sentinel::protocol::{Request, Response};

#[pyclass]
struct SentinelClient {
    identity: String,
    socket_path: String,
}

#[pymethods]
impl SentinelClient {
    #[new]
    fn new(identity: String, socket_path: String) -> Self {
        Self { identity, socket_path }
    }

    /// Autentica al cliente contra el daemon usando un nonce y firma (mock en esta fase)
    fn authenticate(&self) -> PyResult<u64> {
        let mut stream = UnixStream::connect(&self.socket_path)
            .map_err(|e| PyRuntimeError::new_err(format!("No se pudo conectar al Sentinel: {}", e)))?;

        let req = Request::Authenticate {
            identity: self.identity.clone(),
            nonce: [0u8; 32],
            signature: vec![0u8; 3300], // Mock de firma ML-DSA
        };

        let buf = bincode::serialize(&req).map_err(|_| PyRuntimeError::new_err("Error de serialización"))?;
        stream.write_all(&buf).map_err(|_| PyRuntimeError::new_err("Error de escritura"))?;

        let mut resp_buf = vec![0u8; 1024];
        let n = stream.read(&mut resp_buf).map_err(|_| PyRuntimeError::new_err("Error de lectura"))?;
        
        let resp: Response = bincode::deserialize(&resp_buf[..n]).map_err(|_| PyRuntimeError::new_err("Error de deserialización"))?;

        match resp {
            Response::Authenticated { session_handle } => Ok(session_handle),
            Response::Error(e) => Err(PyRuntimeError::new_err(e)),
            _ => Err(PyRuntimeError::new_err("Respuesta inesperada")),
        }
    }

    /// Firma un checkpoint de auditoría (SHA-256) usando la sk del Sentinel
    fn sign_checkpoint(&self, hash_bytes: Vec<u8>) -> PyResult<Vec<u8>> {
        if hash_bytes.len() != 32 {
            return Err(PyRuntimeError::new_err("El hash debe ser de 32 bytes (SHA-256)"));
        }

        let mut stream = UnixStream::connect(&self.socket_path)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

        // Re-autenticación implícita para cada operación táctica (simplificado)
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&hash_bytes);

        let req = Request::SignCheckpoint { hash };
        let buf = bincode::serialize(&req).unwrap();
        stream.write_all(&buf).ok();

        let mut resp_buf = vec![0u8; 4096];
        let n = stream.read(&mut resp_buf).map_err(|_| PyRuntimeError::new_err("Error de lectura"))?;
        let resp: Response = bincode::deserialize(&resp_buf[..n]).unwrap();

        if let Response::Signature(sig) = resp {
            Ok(sig)
        } else {
            Err(PyRuntimeError::new_err("Error al obtener firma del Sentinel"))
        }
    }
}

#[pymodule]
fn helios_pqc(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SentinelClient>()?;
    Ok(())
}
