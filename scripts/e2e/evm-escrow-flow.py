#!/usr/bin/env python3
import json
import os
import subprocess
import sys
import threading
import time
import traceback
import urllib.error
import urllib.parse
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SERVER_URL = os.environ.get("MORPHEUS_E2E_SERVER_URL", "http://127.0.0.1:18180")
MATRIX_URL = os.environ.get("MORPHEUS_E2E_MATRIX_URL", "http://127.0.0.1:18108")
RPC_URL = os.environ.get("MORPHEUS_EVM_RPC_URL", "http://127.0.0.1:8545")
DATABASE_URL = os.environ.get(
    "MORPHEUS_E2E_DATABASE_URL",
    "postgres://morpheus:morpheus@localhost:5432/morpheus_evm_e2e",
)
ADMIN_TOKEN = os.environ.get("MORPHEUS_ADMIN_TOKEN", "admin-token")
SELLER_TOKEN = os.environ.get("MORPHEUS_SELLER_TOKEN", "seller-token")
BUYER_TOKEN = os.environ.get("MORPHEUS_BUYER_TOKEN", "buyer-token")
HOMESERVER_TOKEN = os.environ.get("MORPHEUS_HOMESERVER_TOKEN", "dev-homeserver-token")

ADMIN_KEY = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
BUYER_KEY = "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
SELLER_OPERATOR_KEY = "0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6"

BUYER = "0x70997970C51812dc3A010C7d01b50e0d17dc79C8"
SELLER = "0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC"
SELLER_OPERATOR = "0x90F79bf6EB2c4f870365E785982E1f101E93b906"
ARBITER = "0x15d34AAf54267DB7D7c367839AAf71A00a2C6A65"
ARBITER_OPERATOR = SELLER_OPERATOR

SELLER_ID = "seller:shop.example:01JE2ESELLER"
CUSTOMER_ID = "customer:shop.example:01JE2ECUST"
ARBITER_ID = "arbiter:arbiter.example:01JE2EARB"
PRODUCT_ID = "prod:shop.example:01JE2EPROD"
OFFER_ID = "offer:shop.example:01JE2EOFFER"
ORDER_ID = "ord:shop.example:01JE2EORDER"
PAYMENT_ID = "pay:shop.example:01JE2EPAY"
ENTITLEMENT_ID = "ent:shop.example:01JE2EENT"
ROOM_ID = "!e2e-evm-escrow:shop.example"
AMOUNT_UNITS = "25000000"
PARTIAL_REFUND_UNITS = "10000000"


class MatrixCapture:
    def __init__(self):
        self._events = []
        self._counter = 0
        self._lock = threading.Lock()

    def record_send(self, path, query, content):
        parts = urllib.parse.unquote(path).split("/")
        room_id = parts[5]
        event_type = parts[7]
        sender = content.get("issuer", {}).get("matrix_user_id") or query.get("user_id", ["@market:shop.example"])[0]
        with self._lock:
            self._counter += 1
            event_id = f"$e2e-{self._counter}"
            self._events.append({
                "type": event_type,
                "room_id": room_id,
                "event_id": event_id,
                "sender": sender,
                "origin_server_ts": int(time.time() * 1000),
                "content": content,
            })
        return event_id

    def drain(self):
        with self._lock:
            events = list(self._events)
            self._events.clear()
        return events


def make_matrix_handler(capture):
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, fmt, *args):
            return

        def _json(self, status, payload):
            body = json.dumps(payload).encode()
            self.send_response(status)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def _body(self):
            length = int(self.headers.get("content-length", "0"))
            if length == 0:
                return {}
            return json.loads(self.rfile.read(length).decode())

        def do_POST(self):
            parsed = urllib.parse.urlparse(self.path)
            if parsed.path.startswith("/_matrix/client/v3/join/"):
                self._json(200, {"room_id": urllib.parse.unquote(parsed.path.rsplit("/", 1)[-1])})
                return
            if parsed.path == "/_matrix/client/v3/createRoom":
                body = self._body()
                alias = body.get("room_alias_name", "e2e")
                self._json(200, {"room_id": f"!{alias}:shop.example"})
                return
            self._json(404, {"error": "not found"})

        def do_PUT(self):
            parsed = urllib.parse.urlparse(self.path)
            if "/_matrix/client/v3/rooms/" in parsed.path and "/send/" in parsed.path:
                event_id = capture.record_send(
                    parsed.path,
                    urllib.parse.parse_qs(parsed.query),
                    self._body(),
                )
                self._json(200, {"event_id": event_id})
                return
            self._json(404, {"error": "not found"})

        def do_GET(self):
            parsed = urllib.parse.urlparse(self.path)
            if parsed.path.startswith("/_matrix/client/v3/room/"):
                self._json(200, {"membership": "join"})
                return
            if parsed.path.startswith("/_matrix/client/v3/directory/room/"):
                self._json(200, {"room_id": "!catalog:shop.example"})
                return
            self._json(404, {"error": "not found"})

    return Handler


def http_json(method, path, payload=None, token=None, expect=(200, 202)):
    body = None if payload is None else json.dumps(payload).encode()
    request = urllib.request.Request(
        f"{SERVER_URL}{path}",
        data=body,
        method=method,
        headers={"content-type": "application/json"},
    )
    if token:
        request.add_header("authorization", f"Bearer {token}")
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            data = response.read().decode()
            try:
                result = json.loads(data) if data else {}
            except json.JSONDecodeError as error:
                content_type = response.headers.get("content-type", "")
                raise RuntimeError(
                    f"{method} {path} returned non-json {response.status} "
                    f"content-type={content_type!r} body={data[:500]!r}"
                ) from error
            if response.status not in expect:
                raise RuntimeError(f"{method} {path} returned {response.status}: {result}")
            return result
    except urllib.error.HTTPError as error:
        data = error.read().decode()
        try:
            result = json.loads(data) if data else {}
        except json.JSONDecodeError as decode_error:
            content_type = error.headers.get("content-type", "")
            raise RuntimeError(
                f"{method} {path} returned non-json {error.code} "
                f"content-type={content_type!r} body={data[:500]!r}"
            ) from decode_error
        raise RuntimeError(f"{method} {path} returned {error.code}: {result}") from error


def ingest_events(capture, txn_counter):
    events = capture.drain()
    if not events:
        return txn_counter
    txn_counter += 1
    body = json.dumps({"events": events}).encode()
    url = f"{SERVER_URL}/_matrix/app/v1/transactions/e2e-{txn_counter}?access_token={HOMESERVER_TOKEN}"
    request = urllib.request.Request(url, data=body, method="PUT", headers={"content-type": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            if response.status != 200:
                data = response.read().decode()
                raise RuntimeError(f"appservice transaction returned {response.status}: {data}")
            response.read()
    except urllib.error.HTTPError as error:
        data = error.read().decode()
        raise RuntimeError(f"appservice transaction returned {error.code}: {data}") from error
    return txn_counter


def run(command, env=None):
    printable = " ".join(command)
    print(f"+ {printable}", flush=True)
    subprocess.run(command, cwd=ROOT, env=env, check=True)


def mine_confirmations():
    run(["cast", "rpc", "anvil_mine", "1", "--rpc-url", RPC_URL])


def replay_evm_watcher():
    result = http_json("POST", "/admin/evm-escrow/replay", payload={}, token=ADMIN_TOKEN, expect=(200,))
    print(f"evm replay={json.dumps(result, sort_keys=True)}", flush=True)
    return result


def wait_server(server):
    for _ in range(60):
        if server.poll() is not None:
            raise RuntimeError(f"morpheus-server exited with {server.returncode}")
        try:
            with urllib.request.urlopen(f"{SERVER_URL}/healthz", timeout=2) as response:
                body = response.read().decode()
                payload = json.loads(body) if body else {}
                if response.status == 200 and payload.get("status") == "ok":
                    return
        except Exception:
            time.sleep(1)
    raise RuntimeError("morpheus-server did not become ready")


def patch_config(deployment):
    template = (ROOT / "config/e2e/evm-escrow.toml").read_text()
    patched = template.replace(
        'escrow_contract = "0x0000000000000000000000000000000000000001"',
        f'escrow_contract = "{deployment["escrow_contract"]}"',
    )
    patched = patched.replace(
        'default_token = "0x0000000000000000000000000000000000000002"',
        f'default_token = "{deployment["default_token"]}"',
    )
    patched = patched.replace(
        'contract = "0x0000000000000000000000000000000000000002"',
        f'contract = "{deployment["mock_erc20"]}"',
    )
    patched = patched.replace(
        "start_block = 0",
        f"start_block = {max(int(deployment.get('deploy_block', 0)) - 1, 0)}",
    )
    patched = patched.replace("poll_interval_secs = 1", "poll_interval_secs = 60")
    patched = patched.replace(
        'url = "postgres://morpheus:morpheus@localhost:5432/morpheus"',
        f'url = "{DATABASE_URL}"',
    )
    out = ROOT / ".local/e2e/evm-escrow.toml"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(patched)
    return out


def submit_and_ingest(capture, txn_counter, method, path, payload, token):
    result = http_json(method, path, payload, token)
    return result, ingest_events(capture, txn_counter)


def poll_order(capture, txn_counter, order_id, wanted_status, timeout=45):
    deadline = time.time() + timeout
    while time.time() < deadline:
        txn_counter = ingest_events(capture, txn_counter)
        order = http_json("GET", f"/admin/orders/{urllib.parse.quote(order_id, safe='')}", token=ADMIN_TOKEN)["order"]
        status = order.get("status")
        payment_status = (order.get("payment") or {}).get("status")
        print(f"order_id={order_id} order={status} payment={payment_status}", flush=True)
        if status == wanted_status or payment_status == wanted_status:
            return order, txn_counter
        time.sleep(1)
    raise RuntimeError(f"order {order_id} did not reach {wanted_status}")


def ids_for(suffix):
    return {
        "customer_id": f"{CUSTOMER_ID}{suffix}",
        "product_id": f"{PRODUCT_ID}{suffix}",
        "offer_id": f"{OFFER_ID}{suffix}",
        "order_id": f"{ORDER_ID}{suffix}",
        "payment_id": f"{PAYMENT_ID}{suffix}",
        "entitlement_id": f"{ENTITLEMENT_ID}{suffix}",
        "room_id": f"!e2e-evm-escrow-{suffix.lower()}:shop.example",
        "snapshot_id": f"snap:shop.example:01JE2ESNAP{suffix}",
    }


def create_order_and_intent(capture, txn_counter, ids):
    order_id = ids["order_id"]
    _, txn_counter = submit_and_ingest(capture, txn_counter, "POST", "/api/v1/seller/products", {
        "seller_id": SELLER_ID,
        "product_id": ids["product_id"],
        "revision": 1,
        "title": f"EVM Escrow E2E {order_id.rsplit('ORDER', 1)[-1]}",
        "description": "Local escrow E2E product",
        "kind": "digital_service",
        "categories": ["e2e"],
        "tags": ["evm"],
        "terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
    }, SELLER_TOKEN)
    _, txn_counter = submit_and_ingest(capture, txn_counter, "POST", "/api/v1/seller/offers", {
        "seller_id": SELLER_ID,
        "product_id": ids["product_id"],
        "offer_id": ids["offer_id"],
        "revision": 1,
        "price": {"amount": "25.00", "currency": "USDC"},
        "payment_capture_policy": "before_entitlement",
        "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
        "entitlement_type": "external_entitlement",
        "availability_mode": "unlimited",
    }, SELLER_TOKEN)
    _, txn_counter = submit_and_ingest(capture, txn_counter, "POST", "/api/v1/buyer/orders", {
        "customer_id": ids["customer_id"],
        "customer_display_name": "E2E Buyer",
        "order_id": order_id,
        "room_id": ids["room_id"],
        "seller_id": SELLER_ID,
        "offer_id": ids["offer_id"],
        "offer_revision": 1,
        "catalog_snapshot_id": ids["snapshot_id"],
        "price": {"amount": "25.00", "currency": "USDC"},
        "payment_adapter": "evm_escrow",
        "payment_capture_policy": "before_entitlement",
        "entitlement_type": "external_entitlement",
        "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
        "arbiter_instance": "arbiter.example",
        "arbiter_actor": ARBITER_ID,
        "arbitration_policy_id": "standard-digital-v1",
        "arbitration_policy_version": "1",
        "arbitration_window": "P14D",
        "expires_at": "2027-01-01T00:00:00Z",
    }, BUYER_TOKEN)
    _, txn_counter = submit_and_ingest(capture, txn_counter, "POST", f"/api/v1/seller/orders/{urllib.parse.quote(order_id, safe='')}/accept", {
        "actor_id": SELLER_ID,
        "offer_revision": 1,
        "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
        "payment_capture_policy": "before_entitlement",
        "arbitration_policy_version": "1",
    }, SELLER_TOKEN)
    _, txn_counter = submit_and_ingest(capture, txn_counter, "POST", f"/api/v1/seller/orders/{urllib.parse.quote(order_id, safe='')}/evm-escrow/payment-intent", {
        "actor_id": SELLER_ID,
        "payment_id": ids["payment_id"],
        "buyer_evm_address": BUYER,
        "seller_evm_address": SELLER,
        "arbiter_evm_address": ARBITER,
    }, SELLER_TOKEN)

    order = http_json("GET", f"/admin/orders/{urllib.parse.quote(order_id, safe='')}", token=ADMIN_TOKEN)["order"]
    return order["payment"]["body"]["confirmation"]["order_hash"], txn_counter


def deposit_and_authorize(capture, txn_counter, token, escrow, ids, order_hash):
    run(["cast", "send", token, "mint(address,uint256)", BUYER, AMOUNT_UNITS, "--private-key", ADMIN_KEY, "--rpc-url", RPC_URL])
    run(["cast", "send", token, "approve(address,uint256)", escrow, AMOUNT_UNITS, "--private-key", BUYER_KEY, "--rpc-url", RPC_URL])
    run(["cast", "send", escrow, "deposit(bytes32,address,uint256,address,address,address)", order_hash, token, AMOUNT_UNITS, SELLER, BUYER, ARBITER, "--private-key", BUYER_KEY, "--rpc-url", RPC_URL])
    mine_confirmations()
    replay_evm_watcher()
    return poll_order(capture, txn_counter, ids["order_id"], "authorized")


def run_release_flow(capture, txn_counter, token, escrow):
    ids = ids_for("REL")
    order_hash, txn_counter = create_order_and_intent(capture, txn_counter, ids)
    _, txn_counter = deposit_and_authorize(capture, txn_counter, token, escrow, ids, order_hash)

    run(["cast", "send", escrow, "release(bytes32)", order_hash, "--private-key", SELLER_OPERATOR_KEY, "--rpc-url", RPC_URL])
    mine_confirmations()
    replay_evm_watcher()
    _, txn_counter = poll_order(capture, txn_counter, ids["order_id"], "captured")

    _, txn_counter = submit_and_ingest(capture, txn_counter, "POST", f"/api/v1/seller/orders/{urllib.parse.quote(ids['order_id'], safe='')}/entitlement-grant", {
        "actor_id": SELLER_ID,
        "payment_id": ids["payment_id"],
        "entitlement_id": ids["entitlement_id"],
        "entitlement_type": "external_entitlement",
        "external_ref": "https://shop.example/e2e/entitlement",
        "evidence": {
            "kind": "e2e",
            "uri": "https://shop.example/e2e/evidence",
            "sha256": "sha256:3333333333333333333333333333333333333333333333333333333333333333",
        },
    }, SELLER_TOKEN)

    _, txn_counter = submit_and_ingest(capture, txn_counter, "POST", f"/api/v1/seller/orders/{urllib.parse.quote(ids['order_id'], safe='')}/complete", {
        "actor_id": SELLER_ID,
    }, SELLER_TOKEN)
    _, txn_counter = poll_order(capture, txn_counter, ids["order_id"], "completed")
    return txn_counter


def run_refund_flow(capture, txn_counter, token, escrow):
    ids = ids_for("REF")
    order_hash, txn_counter = create_order_and_intent(capture, txn_counter, ids)
    _, txn_counter = deposit_and_authorize(capture, txn_counter, token, escrow, ids, order_hash)

    run(["cast", "send", escrow, "refund(bytes32)", order_hash, "--private-key", SELLER_OPERATOR_KEY, "--rpc-url", RPC_URL])
    mine_confirmations()
    replay_evm_watcher()
    _, txn_counter = poll_order(capture, txn_counter, ids["order_id"], "refunded")
    return txn_counter


def run_partial_refund_flow(capture, txn_counter, token, escrow):
    ids = ids_for("PART")
    order_hash, txn_counter = create_order_and_intent(capture, txn_counter, ids)
    _, txn_counter = deposit_and_authorize(capture, txn_counter, token, escrow, ids, order_hash)

    run(["cast", "send", escrow, "partial_refund(bytes32,uint256)", order_hash, PARTIAL_REFUND_UNITS, "--private-key", SELLER_OPERATOR_KEY, "--rpc-url", RPC_URL])
    mine_confirmations()
    replay_evm_watcher()
    order, txn_counter = poll_order(capture, txn_counter, ids["order_id"], "refunded")
    amount = (order.get("payment") or {}).get("body", {}).get("amount")
    if amount != "10.00":
        raise RuntimeError(f"partial refund amount mismatch: expected 10.00, got {amount}")
    return txn_counter


def main():
    deployment_path = ROOT / "contracts/deployments/local.json"
    deployment = json.loads(deployment_path.read_text())
    config_path = patch_config(deployment)

    capture = MatrixCapture()
    matrix_endpoint = urllib.parse.urlparse(MATRIX_URL)
    matrix = ThreadingHTTPServer(
        (matrix_endpoint.hostname or "127.0.0.1", matrix_endpoint.port or 80),
        make_matrix_handler(capture),
    )
    thread = threading.Thread(target=matrix.serve_forever, daemon=True)
    thread.start()

    env = os.environ.copy()
    env.update({
        "MORPHEUS_ADMIN_TOKEN": ADMIN_TOKEN,
        "MORPHEUS_SELLER_TOKEN": SELLER_TOKEN,
        "MORPHEUS_BUYER_TOKEN": BUYER_TOKEN,
        "MORPHEUS_EVM_RPC_URL": RPC_URL,
    })
    log_path = ROOT / ".local/e2e/morpheus-server-evm.log"
    log_path.parent.mkdir(parents=True, exist_ok=True)
    with log_path.open("w") as log:
        server = subprocess.Popen(
            ["cargo", "run", "-p", "morpheus-server", "--", "--config", str(config_path)],
            cwd=ROOT,
            env=env,
            stdout=log,
            stderr=subprocess.STDOUT,
        )
    txn_counter = 0
    try:
        wait_server(server)
        print(f"morpheus-server ready at {SERVER_URL}", flush=True)

        _, txn_counter = submit_and_ingest(capture, txn_counter, "POST", "/api/v1/seller/announce", {
            "seller_id": SELLER_ID,
            "display_name": "E2E Seller",
            "legal_profile_ref": "https://shop.example/legal",
            "terms_ref": "https://shop.example/terms",
            "terms_hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "supported_payment_adapters": ["evm_escrow"],
            "supported_entitlement_types": ["external_entitlement"],
        }, SELLER_TOKEN)
        _, txn_counter = submit_and_ingest(capture, txn_counter, "POST", "/api/v1/seller/products", {
            "seller_id": SELLER_ID,
            "product_id": PRODUCT_ID,
            "revision": 1,
            "title": "EVM Escrow E2E",
            "description": "Local escrow E2E product",
            "kind": "digital_service",
            "categories": ["e2e"],
            "tags": ["evm"],
            "terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        }, SELLER_TOKEN)
        _, txn_counter = submit_and_ingest(capture, txn_counter, "POST", "/api/v1/seller/offers", {
            "seller_id": SELLER_ID,
            "product_id": PRODUCT_ID,
            "offer_id": OFFER_ID,
            "revision": 1,
            "price": {"amount": "25.00", "currency": "USDC"},
            "payment_capture_policy": "before_entitlement",
            "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
            "entitlement_type": "external_entitlement",
            "availability_mode": "unlimited",
        }, SELLER_TOKEN)
        token = deployment["mock_erc20"]
        escrow = deployment["escrow_contract"]
        run(["cast", "send", escrow, "set_seller_operator(address,bool)", SELLER_OPERATOR, "true", "--private-key", ADMIN_KEY, "--rpc-url", RPC_URL])
        run(["cast", "send", escrow, "set_arbiter(address,bool)", ARBITER_OPERATOR, "true", "--private-key", ADMIN_KEY, "--rpc-url", RPC_URL])
        txn_counter = run_release_flow(capture, txn_counter, token, escrow)
        txn_counter = run_refund_flow(capture, txn_counter, token, escrow)
        txn_counter = run_partial_refund_flow(capture, txn_counter, token, escrow)
        print("evm escrow e2e ok", flush=True)
    finally:
        server.terminate()
        try:
            server.wait(timeout=10)
        except subprocess.TimeoutExpired:
            server.kill()
        matrix.shutdown()


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(f"evm escrow e2e failed: {error}", file=sys.stderr)
        traceback.print_exc()
        sys.exit(1)
