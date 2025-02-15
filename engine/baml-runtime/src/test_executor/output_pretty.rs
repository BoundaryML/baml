pub(super) struct PrettyTestExecutionStatusRenderer;

use super::{RenderTestExecutionStatus, TestExecutionStatusMap};

impl RenderTestExecutionStatus for PrettyTestExecutionStatusRenderer {
    fn render_progress(&self, test_status_map: &TestExecutionStatusMap) {
        todo!()
    }

    fn render_final(&self, test_status_map: &TestExecutionStatusMap) {
        todo!()
    }
}
