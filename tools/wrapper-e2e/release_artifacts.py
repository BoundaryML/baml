"""Stage the exact uploaded release binaries for the shared conformance suite."""
from pathlib import Path
import tarfile
import zipfile


def candidates_from_archives(artifacts, destination, platform, version):
    target = platform['triple']
    suffix = '.exe' if platform['os'] == 'windows' else ''
    binary_name = 'bin/baml' + suffix
    candidates = []
    for implementation, product in [('rust', 'baml-wrapper'), ('baml', 'baml-wrapper-baml')]:
        for variant in platform['artifacts']['wrapper']['variants']:
            enabled = variant == 'self-update'
            label = implementation + ('' if enabled else '-no-self-update')
            archive_name = f'{product}{"" if enabled else "-no-self-update"}-{version}-{target}{platform["archive_suffix"]}'
            matches = list(Path(artifacts).rglob(archive_name))
            if len(matches) != 1:
                raise ValueError(f'expected exactly one {archive_name}, found {len(matches)}')
            archive = matches[0]
            if archive.suffix == '.zip':
                with zipfile.ZipFile(archive) as source:
                    members = [entry for entry in source.infolist() if entry.filename.removeprefix('./') == binary_name and not entry.is_dir()]
                    if len(members) != 1:
                        raise ValueError(f'{archive}: expected one {binary_name}')
                    data = source.read(members[0])
            else:
                with tarfile.open(archive) as source:
                    members = [entry for entry in source.getmembers() if entry.name.removeprefix('./') == binary_name and entry.isfile()]
                    if len(members) != 1:
                        raise ValueError(f'{archive}: expected one regular {binary_name}')
                    with source.extractfile(members[0]) as binary:
                        data = binary.read()
            # Read only the executable; never extract archive-controlled paths.
            staged = Path(destination) / (label + suffix)
            staged.write_bytes(data)
            staged.chmod(0o755)
            candidates.append({'label': label, 'command': [str(staged.resolve())], 'self_update': enabled, 'version': version, 'posthog': True})
    return candidates
