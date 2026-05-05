#!/usr/bin/env python3
import argparse
import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Optional

HASH = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
SELLER_TERMS = "sha256:1111111111111111111111111111111111111111111111111111111111111111"
OFFER_TERMS = "sha256:2222222222222222222222222222222222222222222222222222222222222222"

INSTANCES = {
    "books": {
        "domain": "books.example",
        "url_env": "MORPHEUS_BOOKS_URL",
        "default_url": "http://127.0.0.1:18081",
        "sellers": ["technical books", "fiction", "children books", "comics", "academic books"],
    },
    "cases": {
        "domain": "cases.example",
        "url_env": "MORPHEUS_CASES_URL",
        "default_url": "http://127.0.0.1:18082",
        "sellers": ["iPhone cases", "Android cases", "rugged cases", "leather cases"],
    },
    "fashion": {
        "domain": "fashion.example",
        "url_env": "MORPHEUS_FASHION_URL",
        "default_url": "http://127.0.0.1:18083",
        "sellers": ["shoes", "sneakers", "boots", "jackets", "shirts"],
    },
}


def load_config(config_dir: Path, name: str) -> dict:
    config: dict[str, dict[str, str]] = {}
    section = ""
    for raw_line in (config_dir / f"{name}.toml").read_text().splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or line.startswith("[["):
            continue
        if line.startswith("[") and line.endswith("]"):
            section = line.strip("[]")
            config.setdefault(section, {})
            continue
        if "=" not in line or not section:
            continue
        key, value = line.split("=", 1)
        value = value.strip()
        if value.startswith('"') and value.endswith('"'):
            value = value[1:-1]
        config[section][key.strip()] = value
    return config


def local(prefix: str, index: int, subindex: int = 0) -> str:
    if subindex:
        return f"{prefix}{index:02d}{subindex:02d}"
    return f"{prefix}{index:02d}"


def envelope(domain: str, event_type: str, room_id: str, sender_domain: str, event_local: str, actor_id: str, body: dict) -> dict:
    return {
        "type": event_type,
        "room_id": room_id,
        "event_id": f"${event_local.lower()}:{sender_domain}",
        "sender": f"@market:{sender_domain}",
        "origin_server_ts": int(time.time() * 1000),
        "content": {
            "protocol": "io.marketplace",
            "protocol_version": "0.1",
            "protocol_event_id": f"evt:{sender_domain}:{event_local}",
            "created_at": "2026-05-04T10:00:00Z",
            "issuer": {
                "instance_id": sender_domain,
                "actor_id": actor_id,
                "matrix_user_id": f"@market:{sender_domain}",
            },
            "critical": [],
            "body": body,
        },
    }


def catalog_events(name: str, domain: str, seller_names: list[str]) -> list[dict]:
    events = []
    room_id = f"!catalog:{domain}"
    for seller_index, seller_name in enumerate(seller_names, start=1):
        seller_id = f"seller:{domain}:{local(name.upper() + 'SELLER', seller_index)}"
        events.append(
            envelope(
                domain,
                "io.marketplace.actor.seller.announced",
                room_id,
                domain,
                local(name.upper() + "SELLEREVT", seller_index),
                seller_id,
                {
                    "seller_id": seller_id,
                    "status": "active",
                    "display_name": seller_name.title(),
                    "legal_profile_ref": f"https://{domain}/legal/{seller_index}",
                    "terms_ref": f"https://{domain}/terms/{seller_index}",
                    "terms_hash": HASH,
                    "supported_payment_adapters": ["mock"],
                    "supported_entitlement_types": ["external_entitlement"],
                },
            )
        )
        for product_index in range(1, 3):
            product_id = f"prod:{domain}:{local(name.upper() + 'PROD', seller_index, product_index)}"
            offer_id = f"offer:{domain}:{local(name.upper() + 'OFFER', seller_index, product_index)}"
            title = f"{seller_name.title()} Item {product_index}"
            events.append(
                envelope(
                    domain,
                    "io.marketplace.product.upserted",
                    room_id,
                    domain,
                    local(name.upper() + "PRODEVT", seller_index, product_index),
                    seller_id,
                    {
                        "product_id": product_id,
                        "seller_id": seller_id,
                        "revision": 1,
                        "status": "active",
                        "kind": "external_entitlement",
                        "title": title,
                        "description": f"Demo catalog item for {seller_name}",
                        "categories": [seller_name],
                        "tags": [name, "demo"],
                        "media": [],
                        "terms_hash": HASH,
                    },
                )
            )
            events.append(
                envelope(
                    domain,
                    "io.marketplace.offer.upserted",
                    room_id,
                    domain,
                    local(name.upper() + "OFFEREVT", seller_index, product_index),
                    seller_id,
                    {
                        "offer_id": offer_id,
                        "product_id": product_id,
                        "seller_id": seller_id,
                        "revision": 1,
                        "status": "active",
                        "price": {"amount": f"{20 + seller_index * 5 + product_index}.00", "currency": "USD"},
                        "payment_terms": {
                            "capture_policy": "before_entitlement",
                            "adapter_policy": "seller_supported",
                        },
                        "seller_terms_hash": SELLER_TERMS,
                        "offer_terms_hash": OFFER_TERMS,
                        "entitlement": {"type": "external_entitlement", "delivery": "external"},
                        "availability": {"mode": "unlimited"},
                    },
                )
            )
    return events


def run_cli(server_url: str, token: str, args: list[str], body: Optional[dict] = None) -> None:
    cli = os.environ.get("MORPHEUS_CLI", "target/debug/morpheus")
    command = [cli, "--server-url", server_url, "--token", token, *args]
    if body is not None:
        command.extend(["--json", json.dumps(body)])
    result = subprocess.run(command, text=True, capture_output=True)
    if result.returncode != 0 and cli == "target/debug/morpheus" and not Path(cli).exists():
        command = ["cargo", "run", "-p", "morpheus-cli", "--", "--server-url", server_url, "--token", token, *args]
        if body is not None:
            command.extend(["--json", json.dumps(body)])
        result = subprocess.run(command, text=True, capture_output=True)
    if result.returncode != 0:
        raise RuntimeError(f"CLI failed: {' '.join(command)}\nstdout={result.stdout}\nstderr={result.stderr}")


def seed_catalog_with_cli(name: str, domain: str, seller_names: list[str], server_url: str) -> None:
    for seller_index, seller_name in enumerate(seller_names, start=1):
        seller_id = f"seller:{domain}:{local(name.upper() + 'SELLER', seller_index)}"
        run_cli(
            server_url,
            "seller-token",
            ["seller", "announce"],
            {
                "seller_id": seller_id,
                "display_name": seller_name.title(),
                "legal_profile_ref": f"https://{domain}/legal/{seller_index}",
                "terms_ref": f"https://{domain}/terms/{seller_index}",
                "terms_hash": HASH,
                "supported_payment_adapters": ["mock"],
                "supported_entitlement_types": ["external_entitlement"],
            },
        )
        for product_index in range(1, 3):
            product_id = f"prod:{domain}:{local(name.upper() + 'PROD', seller_index, product_index)}"
            offer_id = f"offer:{domain}:{local(name.upper() + 'OFFER', seller_index, product_index)}"
            run_cli(
                server_url,
                "seller-token",
                ["seller", "product", "upsert"],
                {
                    "product_id": product_id,
                    "seller_id": seller_id,
                    "revision": 1,
                    "kind": "external_entitlement",
                    "title": f"{seller_name.title()} Item {product_index}",
                    "description": f"Demo catalog item for {seller_name}",
                    "categories": [seller_name],
                    "tags": [name, "demo"],
                    "terms_hash": HASH,
                },
            )
            run_cli(
                server_url,
                "seller-token",
                ["seller", "offer", "upsert"],
                {
                    "offer_id": offer_id,
                    "product_id": product_id,
                    "seller_id": seller_id,
                    "revision": 1,
                    "price": {"amount": f"{20 + seller_index * 5 + product_index}.00", "currency": "USD"},
                    "payment_capture_policy": "before_entitlement",
                    "seller_terms_hash": SELLER_TERMS,
                    "offer_terms_hash": OFFER_TERMS,
                    "entitlement_type": "external_entitlement",
                    "availability_mode": "unlimited",
                },
            )


def order_events() -> list[dict]:
    room_id = "!order-books-fashion:fashion.example"
    customer = "customer:books.example:BOOKCUST01"
    seller = "seller:fashion.example:FASHIONSELLER01"
    offer = "offer:fashion.example:FASHIONOFFER0101"
    order_id = "ord:books.example:BOOKORDER01"
    payment_id = "pay:fashion.example:FASHIONPAY01"
    entitlement_id = "ent:fashion.example:FASHIONENT01"
    return [
        envelope(
            "fashion.example",
            "io.marketplace.actor.customer.bound",
            room_id,
            "books.example",
            "BOOKCUSTBOUND01",
            customer,
            {
                "customer_id": customer,
                "status": "active",
                "display_name": "Books Buyer",
                "instance_id": "books.example",
                "authorized_representatives": ["@market:books.example"],
                "accepted_payment_adapters": ["mock"],
                "accepted_arbitration_policies": ["standard-digital-v1"],
            },
        ),
        envelope(
            "fashion.example",
            "io.marketplace.order.created",
            room_id,
            "books.example",
            "BOOKORDERCREATE01",
            customer,
            {
                "order_id": order_id,
                "room_id": room_id,
                "customer_id": customer,
                "seller_id": seller,
                "offer_id": offer,
                "offer_revision": 1,
                "catalog_snapshot_id": "snap:fashion.example:FASHIONSNAP01",
                "quantity": 1,
                "price": {"amount": "26.00", "currency": "USD"},
                "payment_adapter": "mock",
                "payment_capture_policy": "before_entitlement",
                "entitlement_type": "external_entitlement",
                "seller_terms_hash": SELLER_TERMS,
                "offer_terms_hash": OFFER_TERMS,
                "arbiter_instance": "cases.example",
                "arbiter_actor": "arbiter:cases.example:CASESARBITER01",
                "arbitration_policy_id": "standard-digital-v1",
                "arbitration_policy_version": "1",
                "arbitration_window": "P14D",
                "expires_at": "2026-05-04T10:30:00Z",
            },
        ),
        envelope(
            "fashion.example",
            "io.marketplace.order.accepted",
            room_id,
            "fashion.example",
            "FASHIONORDERACCEPT01",
            seller,
            {
                "order_id": order_id,
                "offer_revision": 1,
                "seller_terms_hash": SELLER_TERMS,
                "offer_terms_hash": OFFER_TERMS,
                "payment_capture_policy": "before_entitlement",
                "arbitration_policy_version": "1",
            },
        ),
        envelope(
            "fashion.example",
            "io.marketplace.payment.intent.created",
            room_id,
            "fashion.example",
            "FASHIONPAYINTENT01",
            seller,
            {
                "order_id": order_id,
                "payment_id": payment_id,
                "adapter": "mock",
                "amount": "26.00",
                "currency": "USD",
                "capture_policy": "before_entitlement",
                "idempotency_key": "idem-fashion-pay-01",
                "provider_ref": "mock_pi_fashion_01",
                "confirmation": {"method": "redirect", "uri": "https://fashion.example/pay/confirm"},
                "expires_at": "2026-05-04T10:30:00Z",
            },
        ),
        envelope("fashion.example", "io.marketplace.payment.authorized", room_id, "fashion.example", "FASHIONPAYAUTH01", seller, {"order_id": order_id, "payment_id": payment_id}),
        envelope(
            "fashion.example",
            "io.marketplace.payment.captured",
            room_id,
            "fashion.example",
            "FASHIONPAYCAPTURE01",
            seller,
            {
                "order_id": order_id,
                "payment_id": payment_id,
                "adapter": "mock",
                "amount": "26.00",
                "currency": "USD",
                "provider_ref": "mock_ch_fashion_01",
                "evidence": {"kind": "receipt", "uri": "https://fashion.example/pay/receipt", "sha256": HASH},
            },
        ),
        envelope(
            "fashion.example",
            "io.marketplace.entitlement.granted",
            room_id,
            "fashion.example",
            "FASHIONENTGRANT01",
            seller,
            {
                "order_id": order_id,
                "payment_id": payment_id,
                "entitlement_id": entitlement_id,
                "type": "external_entitlement",
                "external_ref": "fashion-delivery-01",
                "evidence": {"kind": "delivery", "uri": "https://fashion.example/delivery/01", "sha256": HASH},
            },
        ),
        envelope("fashion.example", "io.marketplace.entitlement.completed", room_id, "fashion.example", "FASHIONENTDONE01", seller, {"order_id": order_id, "entitlement_id": entitlement_id}),
        envelope("fashion.example", "io.marketplace.order.completed", room_id, "fashion.example", "FASHIONORDERDONE01", seller, {"order_id": order_id}),
    ]


def send_transaction(base_url: str, token: str, txn_id: str, events: list[dict], expected: int = 200) -> None:
    payload = json.dumps({"events": events}).encode()
    url = f"{base_url}/_matrix/app/v1/transactions/{txn_id}?access_token={token}"
    request = urllib.request.Request(url, data=payload, method="PUT", headers={"content-type": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            status = response.status
            body = response.read().decode()
    except urllib.error.HTTPError as err:
        status = err.code
        body = err.read().decode()
    if status != expected:
        raise RuntimeError(f"{txn_id}: expected {expected}, got {status}: {body}")


def catalog_items(base_url: str, kind: str) -> list[dict]:
    with urllib.request.urlopen(f"{base_url}/api/v1/catalog/{kind}", timeout=20) as response:
        return json.loads(response.read().decode()).get("items", [])


def wait_local_catalog(server_url: str, domain: str, sellers: int, products: int, offers: int) -> None:
    deadline = time.time() + 90
    while time.time() < deadline:
        seller_count = sum(1 for item in catalog_items(server_url, "sellers") if item.get("issuer_instance") == domain)
        product_count = sum(1 for item in catalog_items(server_url, "products") if f":{domain}:" in item.get("product_id", ""))
        offer_count = sum(1 for item in catalog_items(server_url, "offers") if f":{domain}:" in item.get("offer_id", ""))
        if (seller_count, product_count, offer_count) == (sellers, products, offers):
            return
        time.sleep(1)
    raise RuntimeError(f"{domain} catalog did not project expected local counts")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--config-dir", required=True)
    args = parser.parse_args()
    config_dir = Path(args.config_dir)

    configs = {name: load_config(config_dir, name) for name in INSTANCES}
    for name, meta in INSTANCES.items():
        domain = meta["domain"]
        url = os.environ.get(meta["url_env"], meta["default_url"])
        seed_catalog_with_cli(name, domain, meta["sellers"], url)
        wait_local_catalog(url, domain, len(meta["sellers"]), len(meta["sellers"]) * 2, len(meta["sellers"]) * 2)

    fashion_url = os.environ.get("MORPHEUS_FASHION_URL", INSTANCES["fashion"]["default_url"])
    fashion_token = configs["fashion"]["appservice"]["homeserver_token"]
    order = order_events()
    send_transaction(fashion_url, fashion_token, "fashion-cross-instance-order", order)
    send_transaction(fashion_url, fashion_token, "fashion-cross-instance-order", order)
    send_transaction(fashion_url, fashion_token, "fashion-cross-instance-order", order[:1], expected=409)
    print("demo seed ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
