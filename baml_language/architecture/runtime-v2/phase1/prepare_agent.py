"""Prepare checked Node/native driver inputs without modifying workload sources."""
from pathlib import Path
import shutil

out = Path('target/phase1')
workload = Path('baml_language/tools/speedtest/workloads/programs/agent-loop-300')
project = out / 'agent-native-project'
(project / 'baml_src').mkdir(parents=True, exist_ok=True)
shutil.copyfile(workload / 'main.baml', project / 'baml_src/main.baml')
driver = Path('baml_language/tools/speedtest/src/speedtest/drivers/bench_driver.baml').read_text()
driver = driver.replace('    main();', '    if (string.from(main()) != result) { baml.sys.panic("result mismatch"); }')
(project / 'baml_src/driver.baml').write_text(driver)
# Preserve the shared driver-project identity used for these recorded builds.
(project / 'baml.toml').write_text('[package]\nname = "phase1_richards"\n[pack.env_var_names]\nBAML_TELEMETRY = "BAML_TELEMETRY"\n')
(out / 'agent-checked.js').write_text((workload / 'main.js').read_text() + '\nconst originalMain = main; main = function() { const value = originalMain(); if (String(value) !== "592308425") throw Error("result mismatch"); return value; };\n')
