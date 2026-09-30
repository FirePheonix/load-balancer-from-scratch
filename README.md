# tiny-lb

A Layer-4 TCP load balancer. Clients talk to **one** address. tiny-lb **terminates** that TCP connection, **opens another** to a healthy backend, and **copies bytes**. The kernel still owns TCP. Packets are not forwarded.

Live UI: `http://127.0.0.1:19090`

## How it works in a real system

```mermaid
sequenceDiagram
  participant App as Client app
  participant K1 as Client OS TCP
  participant LB as tiny-lb process
  participant K2 as Backend OS TCP
  participant Svc as Backend app

  App->>K1: write()
  K1->>LB: TCP connection #1 (to VIP :18080)
  Note over LB: accept<br/>skip unhealthy / draining<br/>round-robin pick
  LB->>K2: TCP connection #2 (connect)
  K2->>Svc: accept()
  App->>LB: payload (seq numbers of conn #1)
  LB->>Svc: same bytes (seq numbers of conn #2)
  Note over LB: bounded copy, idle timeout,<br/>half-close via shutdown(WR)
```

Health probes are a **separate** `connect()` loop. They do not sit on the client path. The dashboard on `:19090` only reads the same in-process counters the proxy updates.

## Run

On this Windows box `:8080` is often blocked — use these ports.

Three terminals (fake apps):

```
cargo run -- echo 18001
cargo run -- echo 18002
cargo run -- echo 18003
```

Load balancer:

```
cargo run -- 127.0.0.1:18080 127.0.0.1:18001 127.0.0.1:18002 127.0.0.1:18003
```

Open `http://127.0.0.1:19090`

Scale (real TCP through the LB):

```
cargo run -- load 127.0.0.1:18080 500 30
```
