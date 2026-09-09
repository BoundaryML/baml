//! Invocation authority transported with host callbacks, never recovered from
//! whichever runtime happens to be installed when the host receives them.

use std::sync::{Arc, Weak};

use bex_project::Bex;
use bridge_ctypes::TransferSession;

use crate::BridgeError;

pub struct RuntimeIssuer {
    runtime: Weak<dyn Bex>,
    transfers: TransferSession<'static>,
}

impl RuntimeIssuer {
    pub(crate) fn install(
        runtime: &Arc<dyn Bex>,
        transfers: TransferSession<'static>,
    ) -> Result<(), BridgeError> {
        let issuer = Arc::new(Self {
            runtime: Arc::downgrade(runtime),
            transfers,
        });
        if !runtime.install_host_bridge_context(issuer) {
            return Err(BridgeError::InvalidInvocation(
                "runtime already has a host bridge binding".into(),
            ));
        }
        Ok(())
    }

    /// Upgrade the captured issuer while preparing a host delivery. The
    /// returned session is invocation authority, not the delivery receipt:
    /// arguments already delivered may outlive their original host call.
    pub fn from_host_context(
        context: Option<&Arc<dyn std::any::Any + Send + Sync>>,
    ) -> Result<(Arc<dyn Bex>, TransferSession<'static>), BridgeError> {
        let issuer = context
            .and_then(|context| context.downcast_ref::<Self>())
            .ok_or_else(|| {
                BridgeError::InvalidInvocation("host callback has no issuing SDK runtime".into())
            })?;
        if issuer.transfers.is_closed() {
            return Err(BridgeError::InvalidInvocation(
                "host callback's issuing runtime was closed or replaced".into(),
            ));
        }
        let runtime = issuer.runtime.upgrade().ok_or_else(|| {
            BridgeError::InvalidInvocation(
                "host callback's issuing runtime is no longer available".into(),
            )
        })?;
        Ok((runtime, issuer.transfers.clone()))
    }
}
