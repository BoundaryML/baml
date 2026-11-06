"""Prepare benchmark inputs locally; never edit the spike or workload sources."""
from pathlib import Path
import shutil

out = Path("target/phase1")
workload = Path("baml_language/tools/speedtest/workloads/programs/richards-50")
project = out / "richards-native-project"
(project / "baml_src").mkdir(parents=True, exist_ok=True)
shutil.copyfile(workload / "main.baml", project / "baml_src/main.baml")
(project / "baml.toml").write_text('[package]\nname = "phase1_richards"\n[pack.env_var_names]\nBAML_TELEMETRY = "BAML_TELEMETRY"\n')
driver = Path("baml_language/tools/speedtest/src/speedtest/drivers/bench_driver.baml").read_text()
(project / "baml_src/driver.baml").write_text(driver.replace("    main();", '    if (string.from(main()) != result) { baml.sys.panic("result mismatch"); }'))
(out / "richards-checked.js").write_text((workload / "main.js").read_text() + '\nconst originalMain = main; main = function() { const value = originalMain(); if (String(value) !== "1161046400") throw Error("result mismatch"); return value; };\n')
spike = Path("/home/rathi/Documents/Git/boundary/perf/v2-spike")
(out / "spike").mkdir(exist_ok=True)
for name in ("src", "instrument"):
    shutil.copytree(spike / name, out / "spike" / name, dirs_exist_ok=True)
for name in ("Cargo.toml", "Cargo.lock"):
    shutil.copyfile(spike / name, out / "spike" / name)
