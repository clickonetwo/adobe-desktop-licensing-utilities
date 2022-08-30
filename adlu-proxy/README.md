# adlu-proxy

The ADLU proxy has two main functions:

- It is a protocol-aware, caching, store-forward reverse proxy for applications running under feature-restricted licensing (FRL).  This makes it invaluable for preventing FRL Online packages from escaping their intended environments, as well as making FRL Online licensing available to machines on networks which are intermittently or never connected to the public internet.
- It is a transparent proxy that does log collection and analysis for applications running under named-user licensing (NUL).  This allows administrators to collect statistics about the usage patterns of applications by different named users (whose profiles are separate but anonymous).

