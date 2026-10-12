#!/usr/bin/env python3
"""Verify a deployed Worker without printing credentials or private response bodies."""
import argparse
import json
import os
import urllib.error
import urllib.parse
import urllib.request
from http.cookies import SimpleCookie
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'examples'))
from consumer import NoRedirect


def verify(origin, admin_secret, check_rate_limit=False, check_api_rate_limit=False):
    parsed = urllib.parse.urlsplit(origin)
    if parsed.scheme != 'https' and not (parsed.scheme == 'http' and parsed.hostname in ('localhost', '127.0.0.1')):
        raise ValueError('Use HTTPS for a public service.')
    if parsed.username or parsed.password or parsed.query or parsed.fragment or parsed.path not in ('', '/'):
        raise ValueError('Expected a service origin, without credentials, path or query.')
    origin = origin.rstrip('/')
    opener = urllib.request.build_opener(NoRedirect())
    checks = []

    def request(method, path, value=None, headers=None):
        data = json.dumps(value).encode() if value is not None else None
        req = urllib.request.Request(origin + path, data=data, method=method, headers={'content-type': 'application/json', **(headers or {})})
        try:
            response = opener.open(req, timeout=30)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            raw = response.read()
            status, actual_headers = response.code, response.headers
        body = json.loads(raw) if 'application/json' in actual_headers.get('content-type', '') else raw
        if actual_headers.get('cache-control') != 'no-store':
            raise AssertionError('Service response must not be cached.')
        if actual_headers.get('x-content-type-options') != 'nosniff':
            raise AssertionError('Service response must disable MIME sniffing.')
        return status, actual_headers, body

    status, _, body = request('GET', '/health')
    assert status == 200 and body['data']['status'] == 'ok', 'Liveness check failed.'
    checks.append('liveness')
    status, headers, _ = request('GET', '/dashboard')
    assert status == 200 and "frame-ancestors 'none'" in headers.get('content-security-policy', ''), 'Dashboard CSP missing.'
    if parsed.scheme == 'https':
        assert 'max-age=' in headers.get('strict-transport-security', ''), 'HTTPS transport policy missing.'
    checks.append('dashboard_headers')
    status, _, _ = request('GET', '/ready')
    assert status == 401, 'Readiness must require operator authentication.'
    status, _, body = request('GET', '/ready', headers={'authorization': 'Bearer ' + admin_secret})
    assert status == 200 and body['data']['status'] == 'ready' and body['data']['schema_revision'] == 6, 'D1/R2 readiness check failed.'
    expected_environment = 'production' if parsed.scheme == 'https' else 'development'
    assert body['data']['environment'] == expected_environment, 'Deployment environment is not the expected profile.'
    checks.append('authenticated_d1_r2_readiness')
    status, _, _ = request('POST', '/admin/session', {'admin_secret': admin_secret}, {'origin': 'https://invalid-origin.example'})
    assert status == 403, 'Foreign browser origin was accepted.'
    checks.append('origin_rejection')
    status, headers, body = request('POST', '/admin/session', {'admin_secret': admin_secret}, {'origin': origin})
    assert status == 200, 'Operator login failed.'
    cookie_header = headers.get('set-cookie', '')
    cookies = SimpleCookie(); cookies.load(cookie_header)
    session = cookies['waypost_admin']
    assert session['httponly'] and session['samesite'] == 'Strict', 'Unsafe operator cookie.'
    if parsed.scheme == 'https':
        assert session['secure'], 'Operator cookie must use Secure over HTTPS.'
    csrf = body['data']['csrf_token']
    assert session.value not in csrf, 'CSRF value disclosed session credential.'
    session_headers = {'cookie': 'waypost_admin=' + session.value, 'origin': origin}
    status, _, _ = request('POST', '/admin/v0/registry', {'kind': 'spaces', 'limit': 1}, session_headers)
    assert status == 403, 'Operator write accepted without CSRF.'
    session_headers['x-waypost-csrf'] = csrf
    status, _, _ = request('POST', '/admin/v0/registry', {'kind': 'spaces', 'limit': 1}, session_headers)
    assert status == 200, 'Authenticated registry read failed.'
    status, _, _ = request('DELETE', '/admin/session', {}, session_headers)
    assert status == 200, 'Operator logout failed.'
    status, _, _ = request('GET', '/admin/session', headers=session_headers)
    assert status == 401, 'Logged-out session remained active.'
    checks.append('operator_session_csrf_logout')
    if check_rate_limit:
        for _ in range(12):
            status, _, body = request('POST', '/admin/session', {'admin_secret': 'invalid-test-admin-secret-32-bytes-long'}, {'origin': origin})
            if status == 429:
                assert body['error']['code'] == 'RATE_LIMITED', 'Unexpected rate-limit response.'
                checks.append('login_rate_limit')
                break
            assert status == 401, 'Unexpected login failure status.'
        else:
            raise AssertionError('Login rate limit was not enforced.')
    if check_api_rate_limit:
        for _ in range(320):
            status, _, body = request('GET', '/admin/session', headers={'cookie': 'waypost_admin=invalid_session_read_probe'})
            if status == 429:
                assert body['error']['code'] == 'RATE_LIMITED', 'Unexpected API rate-limit response.'
                checks.append('session_read_api_rate_limit')
                break
            assert status == 401, 'Unexpected session probe response.'
        else:
            raise AssertionError('Session reads bypassed API rate limiting.')
    return {'result': 'passed', 'origin': origin, 'checks': checks}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--url', required=True)
    parser.add_argument('--admin-secret-file', type=Path)
    parser.add_argument('--check-rate-limit', action='store_true', help='Uses this IP login budget; run after other checks.')
    parser.add_argument('--check-api-rate-limit', action='store_true', help='Exercises the local IP API budget; run last.')
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    secret = args.admin_secret_file.read_text().strip() if args.admin_secret_file else os.environ.get('WAYPOST_ADMIN_SECRET')
    if not secret:
        parser.error('Set WAYPOST_ADMIN_SECRET or --admin-secret-file; never pass credentials in the URL.')
    result = verify(args.url, secret, args.check_rate_limit, args.check_api_rate_limit)
    output = json.dumps(result, ensure_ascii=False, indent=2) + '\n'
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(output)
    print(output, end='')

if __name__ == '__main__':
    main()
