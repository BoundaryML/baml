"""Apply the repository's release stamping before building a tagged CLI.

Run only inside the credential-free builder. The tag selects source code;
the official stamp command selects its published product version.
"""
import json
from pathlib import Path
import re
import runpy
import subprocess
import sys
import tempfile


def stamp(repo, version):
    if not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+(?:-nightly\.[0-9]{8}\.[a-z])?', version):
        raise ValueError('invalid published CLI version')
    script = Path(repo) / 'scripts/baml-language-version'
    if not script.is_file():
        # Older tags embed their version without this release script. The
        # builder still checks the resulting --version against the request.
        return
    api = runpy.run_path(str(script))
    plan = {
        'schema': api['PLAN_SCHEMA'],
        'channel': 'nightly' if '-nightly.' in version else 'canary',
        'canary_version': str(api['load_release']()),
        'canonical_version': version,
        'registry_versions': api['registry_versions_for'](version),
        'git_tag': 'baml-language-' + version,
        'released_at': api['released_at_for_plan'](),
    }
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / 'release-plan.json'
        path.write_text(json.dumps(plan))
        subprocess.run([sys.executable, str(script), 'stamp', '--plan', str(path)], check=True, stdout=sys.stderr)


if __name__ == '__main__':
    stamp(sys.argv[1], sys.argv[2])
