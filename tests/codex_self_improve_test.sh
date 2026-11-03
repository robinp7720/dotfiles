#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
python3 - "$ROOT_DIR" <<'PY'
import fcntl
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

root = Path(sys.argv[1])
wrapper = root / 'scripts/codex_self_improve.sh'
FAKE = r'''#!/usr/bin/env python3
import json, os, pathlib, re, shlex, subprocess, sys, time
args = sys.argv[1:]
assert '--full-auto' not in args
assert args[:3] == ['-a', 'never', 'exec']
assert args[args.index('--sandbox') + 1] == 'workspace-write'
assert '--output-schema' in args
if '--help' in args:
    sys.exit(0)
repo = pathlib.Path(args[args.index('-C') + 1])
report = pathlib.Path(args[args.index('-o') + 1])
prompt = sys.stdin.read()
phase = 'commit' if 'commit-only phase' in prompt else 'improve'
commit_options = ['-c', 'commit.gpgsign=false']
if phase == 'commit':
    # Exercise the actual commit recipe supplied to Codex against signed repos.
    recipe = re.search(r'`(git -c commit\.gpgsign=false commit)`', prompt)
    assert recipe, 'commit-only prompt must supply the sandbox-compatible recipe'
    commit_options = shlex.split(recipe.group(1))[1:-1]
case = os.environ['TEST_CASE']
live = pathlib.Path(os.environ['TEST_REPO'])
with open(os.environ['TEST_CALLS'], 'a') as f:
    f.write(json.dumps({'phase': phase, 'cwd': str(repo), 'args': args}) + '\n')
def git(*args, cwd=repo):
    return subprocess.check_output(['git', '-C', str(cwd), *commit_options,
        '-c', 'user.name=Test', '-c', 'user.email=test@localhost', *args], stderr=subprocess.STDOUT)
status, files = 'no_change', []
if phase == 'commit':
    if case == 'commit_failed':
        sys.exit(42)
    if case == 'commit_failed_report':
        report.write_text(json.dumps(dict(status='failed', title='Commit blocked',
            summary='Signing agent unavailable.\nExisting changes preserved.\r\n',
            files=['other.txt'], validation=[])))
        sys.exit(0)
    if case == 'commit_edit':
        (repo / 'README.md').write_text('not authorized by commit phase\n')
    if (repo / 'nested/.git').exists():
        git('add', '-A', cwd=repo/'nested')
        git('commit', '-qm', 'fix(nested): record existing change', cwd=repo/'nested')
    if case != 'commit_dirty':
        # Separate pending changes by file, standing in for semantic grouping.
        paths = git('ls-files', '-m', '-o', '--exclude-standard').decode().splitlines()
        for path in dict.fromkeys(paths):
            git('add', '--', path)
            git('commit', '-qm', 'chore: record ' + path)
        if git('diff', '--cached', '--name-only').strip():
            git('commit', '-qm', 'chore: record staged changes')
    status = 'changed'
else:
    assert repo != live, 'improvement must be isolated'
    assert git('rev-parse', '--git-common-dir').strip() == b'.git'
    if case in ('no_change', 'dirty_first', 'nested_first'):
        pass
    elif case == 'exit_failed':
        sys.exit(42)
    elif case == 'timeout':
        time.sleep(10)
    else:
        (repo / 'README.md').write_text('validated improvement\n')
        status, files = 'changed', ['README.md']
        if case == 'missing_report':
            sys.exit(0)
        if case == 'malformed_report':
            report.write_text('STATUS: changed\n')
            sys.exit(0)
        if case == 'failed_report':
            status = 'failed'
        if case == 'mismatch':
            files = ['wrong.md']
        if case == 'false_no_change':
            status = 'no_change'
        if case == 'concurrent':
            (live / 'README.md').write_text('concurrent user change\n')
        if case == 'head_changed':
            (live / 'user.txt').write_text('user\n')
            git('add', 'user.txt', cwd=live)
            git('commit', '-qm', 'chore: user commit', cwd=live)
        if case == 'invalid_shell':
            (repo / 'scripts').mkdir(exist_ok=True)
            (repo / 'scripts/broken.sh').write_text('#!/bin/bash\nif then\n')
            files.append('scripts/broken.sh')
        if case == 'submodule_edit':
            (repo / 'nested').mkdir(exist_ok=True)
            (repo / 'nested/new.txt').write_text('unsupported\n')
            files.append('nested/new.txt')
        if case == 'symlink':
            (repo / 'docs').mkdir(exist_ok=True)
            (repo / 'docs/link.md').symlink_to(live / 'README.md')
            files.append('docs/link.md')
        if case == 'premature_commit':
            git('add', '-A')
            git('commit', '-qm', 'fix: premature')
        if case == 'policy_edit':
            (repo / 'tools/self-improve').mkdir(parents=True, exist_ok=True)
            (repo / 'tools/self-improve/validate.sh').write_text('exit 0\n')
            files.append('tools/self-improve/validate.sh')
validation = [] if case == 'no_validation' else ['git diff --check']
summary = 'Independent check failed.\nCandidate retained.' if status == 'failed' else 'Updated documentation.'
if case == 'multiline_summary':
    summary += '\n\n'
report.write_text(json.dumps(dict(status=status, title='fix(docs): clarify setup',
    summary=summary, files=files, validation=validation)))
'''

def git(repo, *args):
    return subprocess.check_output(['git', '-C', str(repo), *args], stderr=subprocess.STDOUT).decode().strip()

runs = 0

class Fixture:
    def __init__(self, directory, case):
        self.dir = directory / case
        self.dir.mkdir()
        self.repo = self.dir / 'repo'
        self.repo.mkdir()
        self.case = case
        self.init(self.repo)
        self.before = git(self.repo, 'rev-parse', 'HEAD')
        self.fake = self.dir / 'codex'
        self.fake.write_text(FAKE)
        self.fake.chmod(0o755)
        self.bin = self.dir / 'bin'
        self.bin.mkdir()
        notify = self.bin / 'notify-send'
        notify.write_text('#!/bin/sh\nexit 0\n')
        notify.chmod(0o755)
        self.calls = self.dir / 'calls'
        self.state = self.dir / 'state/codex-self-improve'
    def init(self, repo):
        git(repo, 'init', '-q')
        git(repo, 'config', 'user.name', 'Test')
        git(repo, 'config', 'user.email', 'test@localhost')
        git(repo, 'config', 'commit.gpgsign', 'false')
        (repo / 'README.md').write_text('initial\n')
        git(repo, 'add', '.')
        git(repo, 'commit', '-qm', 'test: initial')
    def run(self, *args, force=True, **overrides):
        global runs
        runs += 1
        env = os.environ.copy()
        for key in list(env):
            if key.startswith('CODEX_SELF_IMPROVE_'):
                del env[key]
        env.update(HOME=str(self.dir/'home'), XDG_STATE_HOME=str(self.dir/'state'),
            XDG_RUNTIME_DIR=str(self.dir/'runtime'), PATH=str(self.bin)+':'+env['PATH'],
            CODEX_SELF_IMPROVE_REPO_DIR=str(self.repo), CODEX_SELF_IMPROVE_CODEX_BIN=str(self.fake),
            CODEX_SELF_IMPROVE_NOTIFY_FAILURES='0', TEST_REPO=str(self.repo),
            TEST_CASE=self.case, TEST_CALLS=str(self.calls))
        env.update(overrides)
        result = subprocess.run(['bash', str(wrapper), *(['--force'] if force else []), *args],
            env=env, capture_output=True, text=True, timeout=20)
        summary = dict(line.split('=', 1) for line in (self.state/'last-summary.txt').read_text().splitlines())
        return result, summary
    def phases(self):
        return [json.loads(x)['phase'] for x in self.calls.read_text().splitlines()] if self.calls.exists() else []
    def assert_failure(self):
        result, summary = self.run(CODEX_SELF_IMPROVE_TIMEOUT_SECONDS='1' if self.case=='timeout' else '10')
        assert result.returncode != 0, (self.case, summary, result.stderr)
        assert summary['status'] == 'failed', (self.case, summary)
        assert git(self.repo, 'rev-parse', 'HEAD') == self.before
        assert not (self.state/'last-run-epoch').exists()
        assert (self.repo/'README.md').read_text() == 'initial\n'
        assert not git(self.repo, 'status', '--porcelain')
        assert self.phases() == ['improve']
        if self.case == 'failed_report':
            assert summary['summary'] == 'improve: Independent check failed. Candidate retained.'

with tempfile.TemporaryDirectory(prefix='codex-self-improve-tests-') as tmp:
    directory = Path(tmp)
    cases = ['exit_failed', 'failed_report', 'missing_report', 'malformed_report',
        'mismatch', 'false_no_change', 'no_validation', 'invalid_shell',
        'submodule_edit', 'symlink', 'premature_commit', 'policy_edit', 'timeout']
    for case in cases:
        Fixture(directory, case).assert_failure()
    for case in ['changed', 'no_change']:
        f = Fixture(directory, case)
        result, summary = f.run()
        assert result.returncode == 0 and summary['status'] == case, (case, result.stderr, summary)
        assert (f.state/'last-run-epoch').exists()
        assert not git(f.repo, 'status', '--porcelain')
        assert (git(f.repo, 'rev-parse', 'HEAD') != f.before) == (case == 'changed')
        assert not Path(summary['checkout']).exists()
        previous = Path(summary['run'])
        _, skipped = f.run(force=False)
        assert skipped['status'] == 'skipped'
        assert previous.joinpath('run.log').exists()
        assert f.phases() == ['improve']
    for case in ['dirty_first', 'nested_first']:
        f = Fixture(directory, case)
        (f.repo/'README.md').write_text('existing user work\n')
        (f.repo/'other.txt').write_text('separate concern\n')
        if case == 'nested_first':
            child = f.dir/'child'
            child.mkdir()
            f.init(child)
            git(f.repo, '-c', 'protocol.file.allow=always', 'submodule', 'add', '-q', str(child), 'nested')
            git(f.repo, 'commit', '-qm', 'test: add submodule')
            (f.repo/'nested/README.md').write_text('nested user work\n')
            git(f.repo/'nested', 'config', 'commit.gpgsign', 'true')
            git(f.repo/'nested', 'config', 'gpg.program', '/bin/false')
        git(f.repo, 'config', 'commit.gpgsign', 'true')
        git(f.repo, 'config', 'gpg.program', '/bin/false')
        result, summary = f.run()
        assert result.returncode == 0, (case, summary, result.stderr, (f.state/'last-run.log').read_text())
        assert f.phases() == ['commit', 'improve']
        assert (f.repo/'README.md').read_text() == 'existing user work\n'
        assert not git(f.repo, 'status', '--porcelain')
        assert int(git(f.repo, 'rev-list', '--count', f.before+'..HEAD')) >= 2
        assert git(f.repo, 'config', 'commit.gpgsign') == 'true'
        if case == 'nested_first':
            assert git(f.repo/'nested', 'config', 'commit.gpgsign') == 'true'
    for case in ['commit_failed', 'commit_failed_report', 'commit_dirty', 'commit_edit']:
        f = Fixture(directory, case)
        (f.repo/'other.txt').write_text('user work\n')
        result, summary = f.run()
        assert result.returncode != 0 and summary['status'] == 'failed'
        assert f.phases() == ['commit']
        if case == 'commit_failed_report':
            assert summary['summary'].rstrip() == 'commit_existing: Signing agent unavailable. Existing changes preserved.', summary
            assert (f.repo/'other.txt').read_text() == 'user work\n'
            assert git(f.repo, 'rev-parse', 'HEAD') == f.before
    # Embedded repositories must be caught before the commit-only agent can touch
    # them, including .git files, unusual paths, and checkouts inside submodules.
    for case in ['unregistered', 'unregistered_worktree', 'unregistered_in_submodule']:
        f = Fixture(directory, case)
        container = f.repo
        if case == 'unregistered_in_submodule':
            source = f.dir/'source'
            source.mkdir()
            f.init(source)
            git(f.repo, '-c', 'protocol.file.allow=always', 'submodule', 'add', '-q', str(source), 'nested')
            git(f.repo, 'commit', '-qm', 'test: add submodule')
            container = f.repo/'nested'
        nested = container/'tools'/'local checkout\nwith newline'
        nested.parent.mkdir()
        if case == 'unregistered_worktree':
            source = f.dir/'source'
            source.mkdir()
            f.init(source)
            git(source, 'worktree', 'add', '-q', '--detach', str(nested))
        else:
            nested.mkdir()
            f.init(nested)
        (nested/'README.md').write_text('unrelated local changes\n')
        (f.repo/'pending.txt').write_text('existing work\n')
        before_head = git(f.repo, 'rev-parse', 'HEAD')
        before_status = git(f.repo, 'status', '--porcelain')
        nested_head = git(nested, 'rev-parse', 'HEAD')
        nested_status = git(nested, 'status', '--porcelain')
        result, summary = f.run()
        assert result.returncode == 0 and summary['status'] == 'skipped', (case, summary, result.stderr)
        assert summary['phase'] == 'preflight' and 'Unregistered Git checkout' in summary['summary']
        assert f.phases() == []
        assert git(f.repo, 'rev-parse', 'HEAD') == before_head
        assert git(f.repo, 'status', '--porcelain') == before_status
        assert git(nested, 'rev-parse', 'HEAD') == nested_head
        assert git(nested, 'status', '--porcelain') == nested_status
        assert (nested/'README.md').read_text() == 'unrelated local changes\n'
        assert not (f.state/'last-run-epoch').exists()
    # Intentionally ignored checkouts remain independent and do not block a run.
    f = Fixture(directory, 'ignored_checkout')
    nested = f.repo/'local'
    nested.mkdir()
    f.init(nested)
    (nested/'README.md').write_text('private work\n')
    with (f.repo/'.git/info/exclude').open('a') as exclude:
        exclude.write('\n/local/\n')
    f.case = 'no_change'
    result, summary = f.run()
    assert result.returncode == 0 and summary['status'] == 'no_change', summary
    assert f.phases() == ['improve']
    assert (nested/'README.md').read_text() == 'private work\n'
    for case in ['concurrent', 'head_changed']:
        f = Fixture(directory, case)
        result, summary = f.run()
        assert result.returncode == 0 and summary['status'] == 'pending_review', summary
        assert Path(summary['checkout']).exists()
        assert f.phases() == ['improve']
        if case == 'concurrent':
            assert (f.repo/'README.md').read_text() == 'concurrent user change\n'
            assert git(f.repo, 'rev-parse', 'HEAD') == f.before
        else:
            assert (f.repo/'README.md').read_text() == 'initial\n'
            assert git(f.repo, 'log', '-1', '--format=%s') == 'chore: user commit'
    f = Fixture(directory, 'no_commit')
    result, summary = f.run('--no-commit')
    assert result.returncode == 0 and summary['status'] == 'pending_review'
    assert git(f.repo, 'rev-parse', 'HEAD') == f.before
    assert (f.repo/'README.md').read_text() == 'initial\n'
    assert (Path(summary['checkout'])/'README.md').read_text() == 'validated improvement\n'
    (f.repo/'dirty.txt').write_text('leave alone\n')
    _, summary = f.run('--no-commit')
    assert summary['status'] == 'skipped' and f.phases() == ['improve']
    f = Fixture(directory, 'invalid_timestamp')
    f.case = 'no_change'
    f.state.mkdir(parents=True)
    (f.state/'last-run-epoch').write_text('invalid timestamp')
    result, summary = f.run(force=False)
    assert result.returncode == 0 and summary['status'] == 'no_change', summary
    (f.state/'last-boot-id').write_text('previous boot')
    _, summary = f.run(force=False)
    assert summary['status'] == 'skipped' and 'Cooldown' in summary['summary']
    f = Fixture(directory, 'multiline_summary')
    result, summary = f.run()
    assert result.returncode == 0 and summary['status'] == 'changed', summary
    f = Fixture(directory, 'disabled')
    _, summary = f.run(CODEX_SELF_IMPROVE_DISABLED='1')
    assert summary['status'] == 'skipped' and f.phases() == []
    # Contending runs cannot replace another run's log or start an agent.
    log_before = (f.state/'last-run.log').read_bytes()
    with open(f.dir/'runtime/codex-self-improve.lock', 'w') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        result, _ = f.run()
    assert result.returncode == 0 and f.phases() == []
    assert (f.state/'last-run.log').read_bytes() == log_before

    # Login settings reach systemd; the optional EnvironmentFile supports persistence.
    launcher_home = directory/'launcher-home'
    launcher_bin = directory/'launcher-bin'
    launcher_bin.mkdir()
    launcher_log = directory/'launcher-log'
    stub = launcher_bin/'systemctl'
    stub.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> "$LAUNCHER_LOG"\n')
    stub.chmod(0o755)
    env = os.environ.copy()
    env.update(HOME=str(launcher_home), PATH=str(launcher_bin)+':'+env['PATH'],
        LAUNCHER_LOG=str(launcher_log), CODEX_SELF_IMPROVE_DISABLED='1',
        CODEX_SELF_IMPROVE_AUTO_COMMIT='0')
    subprocess.run(['bash', str(root/'scripts/start_codex_self_improve_service.sh')], env=env, check=True)
    launcher_calls = launcher_log.read_text()
    assert 'import-environment CODEX_SELF_IMPROVE_DISABLED CODEX_SELF_IMPROVE_AUTO_COMMIT' in launcher_calls
    assert '--user start --no-block codex-self-improve.service' in launcher_calls
print(f'ok - {runs} self-improve runs passed (real Git, fake Codex, isolated repositories)')
PY
