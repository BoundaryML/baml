import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;

class TestSample {
    void helper() {}

    @Test
    void test_sync_case() {}

    @Disabled("fixture")
    @Test
    void test_disabled_case() throws Exception {}
}
