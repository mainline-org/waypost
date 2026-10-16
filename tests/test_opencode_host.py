"""The opt-in CLI must distinguish host reachability from a connected Waypost MCP."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from test_backup_restore import NativeService


ROOT = Path(__file__).resolve().parents[1]


class OpenCodeHostEvidence(unittest.TestCase):
    def check_connection(self, inventory):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            service = NativeService(root / 'data').start()
            try:
                binary = root / 'opencode-fixture'
                binary.write_text(f'''#!{sys.executable}
import json, sys
from http.server import BaseHTTPRequestHandler, HTTPServer
if '--version' in sys.argv:
    print('2.0.20')
    sys.exit(0)
inventory = {inventory!r}
class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        value = inventory if self.path == '/api/mcp' else {{}}
        body = json.dumps(value).encode()
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *args):
        pass
HTTPServer(('127.0.0.1', int(sys.argv[sys.argv.index('--port') + 1])), Handler).serve_forever()
''')
                binary.chmod(0o700)
                artifacts = root / 'artifacts'
                artifacts.mkdir(mode=0o700)
                env = {key: value for key, value in os.environ.items()
                       if not key.startswith(('WAYPOST_', 'OPENCODE_'))}
                env['WAYPOST_ADMIN_SECRET'] = (service.data / 'admin-secret').read_text().strip()
                env['TMPDIR'] = str(artifacts)
                result = subprocess.run(
                    [sys.executable, str(ROOT / 'scripts/test_opencode_agents.py'),
                     '--url', service.origin, '--binary', str(binary), '--connect-only'],
                    env=env, capture_output=True, text=True, timeout=30)
                events = [json.loads(line) for line in result.stdout.splitlines()]
                return result.returncode, events, result.stderr
            finally:
                service.stop()

    def test_an_empty_host_inventory_is_not_a_successful_mcp_connection(self):
        code, events, error = self.check_connection({'location': {'id': 'fixture'}, 'data': []})
        self.assertNotEqual(code, 0)
        self.assertFalse(any(event.get('host_connection') == 'passed' for event in events))
        self.assertIn('Actual OpenCode MCP connection failed', error)

    def test_another_connected_server_cannot_stand_in_for_waypost(self):
        code, events, error = self.check_connection({
            'location': {'id': 'fixture'},
            'data': {'other': {'status': 'connected'}, 'waypost': {'status': 'disconnected'}}})
        self.assertNotEqual(code, 0)
        self.assertFalse(any(event.get('host_connection') == 'passed' for event in events))
        self.assertIn('Actual OpenCode MCP connection failed', error)

    def test_the_named_waypost_server_can_report_a_connected_state(self):
        code, events, error = self.check_connection({
            'location': {'id': 'fixture'},
            'data': [{'name': 'waypost', 'status': {'_tag': 'Connected'}}]})
        self.assertEqual(code, 0, error)
        self.assertTrue(any(event.get('host_connection') == 'passed' for event in events))


if __name__ == '__main__':
    unittest.main()
