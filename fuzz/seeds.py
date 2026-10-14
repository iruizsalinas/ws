"""Seed coverage-guided fuzzing with valid states and structured violations."""
from pathlib import Path

ROOT = Path(__file__).resolve().parent / 'corpus'
KEY = 'dGhlIHNhbXBsZSBub25jZQ=='


def seed(target, name, data):
    folder = ROOT / target
    folder.mkdir(parents=True, exist_ok=True)
    (folder / name).write_bytes(data)


def frame(op, data=b'', masked=False, final=True):
    assert len(data) <= 125
    head = bytes([op | (0x80 if final else 0), len(data) | (0x80 if masked else 0)])
    if masked:
        key = b'\x01\x02\x03\x04'
        return head + key + bytes(value ^ key[i % 4] for i, value in enumerate(data))
    return head + data


def generate():
    request = (f'GET /chat?q=1 HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\n'
               f'Connection: Upgrade\r\nSec-WebSocket-Key: {KEY}\r\nSec-WebSocket-Version: 13\r\n')
    response = ('HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n'
                'Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\nSec-WebSocket-Protocol: chat\r\n\r\n')
    for name, text in {
        'request': request + '\r\n',
        'protocols': request + 'Sec-WebSocket-Protocol: ' + ','.join(f'p{i}' for i in range(2000)) + '\r\n\r\n',
        'duplicate-origin': request + 'Origin: https://a\r\nOrigin: https://b\r\n\r\n',
        'long-key': request.replace(KEY, 'A' * 12000) + '\r\n',
        'response': response,
        'url-ipv6': 'wss://[::1]:8443/path?q=1',
        'url': 'ws://localhost:8080/path',
    }.items():
        seed('handshake', name, text.encode())
    for masked in [False, True]:
        role = bytes([1 if masked else 0])
        cases = {
            'text': frame(1, b'hello', masked),
            'binary': frame(2, bytes(range(125)), masked),
            'close': frame(8, b'\x03\xe8done', masked),
            'utf8-fragments': frame(1, b'\xe2', masked, False) + frame(9, b'ping', masked) + frame(0, b'\x82\xac', masked),
            'bad-utf8': frame(1, b'\xff', masked),
            'bad-sequence': frame(0, b'continuation', masked),
        }
        for name, data in cases.items():
            seed('reader', f'{int(masked)}-{name}', role + data)


if __name__ == '__main__':
    generate()
