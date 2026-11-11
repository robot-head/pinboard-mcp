# Configurable identity and email

Pinboard supports one OIDC provider, one SAML 2.0 IdP, or both at the same time. The login screen uses configurable labels. Without either provider, the server runs as a trusted local owner and refuses public hosting.

## Hosting

Set `PINBOARD_PUBLIC_URL` to the canonical HTTPS origin, for example `https://canvas.example.com`. Terminate TLS at a reverse proxy, preserve the public `Host` header, and proxy WebSocket upgrades. Bind the Rust listener to loopback when the proxy is on the same machine. Host/Origin checks allow the configured public origin and local listener; arbitrary origins are rejected.

Configure `PINBOARD_ADMIN_EMAILS` and `PINBOARD_INTERNAL_DOMAINS` as comma-separated values. Admins can manage organization policy and all canvases. Internal members can create boards; external users can access only boards they are invited to. Keep these values in server configuration. Roles are recomputed from configuration on authentication.

Use a single server process per embedded Turso database. Pending SSO requests and MCP transport sessions are process-local: if running multiple servers against Turso Cloud, use sticky routing for each sign-in/MCP session. Restarting the server invalidates unfinished sign-ins; existing durable browser/API sessions survive.

## OIDC

Set `PINBOARD_OIDC_ISSUER`, `PINBOARD_OIDC_CLIENT_ID`, and, if required by your provider, `PINBOARD_OIDC_CLIENT_SECRET`. Optionally set `PINBOARD_OIDC_LABEL`.

Register `${PINBOARD_PUBLIC_URL}/auth/callback` as an exact redirect URI. Discovery must return the configured issuer and HTTPS authorization/token/JWKS endpoints (loopback HTTP is permitted for development). The integration uses authorization code flow, PKCE S256, nonce, and a browser-bound one-use state. It validates asymmetric token signatures, issuer, audience, authorized party (including multiple-audience tokens), expiry, and nonce. A verified email is required in the ID token or matching-subject UserInfo response. Providers that do not assert `email_verified: true` need an appropriate claim mapping.

## SAML 2.0

Set either `PINBOARD_SAML_IDP_METADATA_FILE` or `PINBOARD_SAML_IDP_METADATA_URL`. A file takes precedence. Use trusted metadata containing the exact IdP entity ID, signing certificates, and an HTTP-Redirect SSO endpoint. HTTPS is required for remote metadata. The metadata is loaded at startup; restart after metadata/certificate rotation. Expired metadata is rejected. Keep metadata configuration restricted to administrators; it establishes the trust root.

Provide `PINBOARD_SAML_SP_KEY_FILE` and `PINBOARD_SAML_SP_CERT_FILE` pointing to matching PEM files. The signing key must be RSA with at least 2048 bits. Mount the private key from your secret manager, readable only by the service account. For a development certificate:

```sh
openssl req -x509 -newkey rsa:3072 -nodes \
  -keyout pinboard-saml-key.pem -out pinboard-saml-cert.pem \
  -days 365 -subj '/CN=Pinboard SAML'
chmod 600 pinboard-saml-key.pem
```

The SP certificate signs SAML requests; it is separate from the site's TLS certificate. Upload the SP metadata from `${PINBOARD_PUBLIC_URL}/auth/saml/metadata` into your IdP. It advertises signed requests, the signing certificate, and HTTP-POST ACS. No logout endpoint is advertised.

| IdP registration | Value |
| --- | --- |
| SP entity ID / audience | `${PINBOARD_PUBLIC_URL}/auth/saml/metadata`, or `PINBOARD_SAML_ENTITY_ID` |
| ACS / recipient / destination | `${PINBOARD_PUBLIC_URL}/auth/saml/acs` |
| Request binding | HTTP-Redirect, signed with RSA-SHA256 |
| Response binding | HTTP-POST |
| NameID | Stable persistent identifier by default |
| Email attribute | `email`, configurable with `PINBOARD_SAML_EMAIL_ATTRIBUTE` |
| Display name attribute | `displayName`, configurable with `PINBOARD_SAML_NAME_ATTRIBUTE` |

Set `PINBOARD_SAML_LABEL` for the login button. `PINBOARD_SAML_NAME_ID_FORMAT` changes the requested NameID format. Use a stable identifier rather than transient NameIDs. Attribute names or FriendlyNames may match; email must contain exactly one valid address. Configure the email mapping as an authoritative, verified address in the IdP: a signed SAML email is trusted for account linking and invitation acceptance.

Only SP-initiated sign-in is supported. Each request has a random ID and a browser-bound, one-use RelayState with a five-minute lifetime. HTTPS uses an HttpOnly `SameSite=None; Secure` correlation cookie so cross-site POST works. Loopback HTTP is intended only for same-site development; use local HTTPS when testing a cross-site IdP.

Samael/xmlsec verifies signed XML before identity attributes are consumed. The server requires trusted signature coverage, matching issuer/audience/destination/recipient/request ID, bounded issue time, expiring conditions and bearer subject confirmation, and SHA-256-or-stronger signature/digest algorithms. All audience restrictions must match. DTDs, entities, external references, key retrieval, and nonstandard transforms are rejected. Unsolicited, replayed, expired, unsigned, or tampered responses fail authentication. Assertion expiry allows 30 seconds of clock skew; keep server and IdP clocks synchronized.

Encrypted assertions and IdP-initiated login are not part of this integration. Sign out revokes the Pinboard session; it does not terminate the IdP session or implement SAML Single Logout.

## Accounts, sessions, and MCP

OIDC and SAML identities with the same trusted verified email link to one durable account, preserving board grants and discussions. Subsequent logins retain the original issuer/subject binding. A changed email cannot take over an address bound to another account. Only connect identity providers authoritative for your organization and invited guests.

Browser sessions use random opaque HttpOnly cookies, expire after 12 hours, and are stored as hashes. Sign out revokes the session. Personal MCP tokens expire after 30 days, are shown once, and can be revoked in Account settings. HTTP MCP accepts `Authorization: Bearer ...`; authenticated stdio uses `PINBOARD_STDIO_TOKEN`. Tokens inherit canvas roles and organization policy. WebSocket sessions recheck authentication and grants every two seconds and disconnect on revocation/expiry.

## SMTP and sharing

Configure `PINBOARD_SMTP_HOST`, `PINBOARD_SMTP_FROM`, and optionally port/username/password. `PINBOARD_SMTP_TLS` accepts `starttls` (default, port 587), `tls` (default port 465), or `none` (loopback testing only). A configurable TLS SMTP service receives invitations; no vendor-specific email API is required.

Owners invite editors, commenters, or viewers through Share. Each token is hashed, expires after seven days, and is usable only by its recipient's verified account after successful delivery. Failed sends leave a revoked invitation. Replaying an accepted invitation cannot restore revoked access or revert a changed role. External invitations are disabled by default; admins can enable them under **Account → Organization settings → Artifacts**, or disable the canvas feature there. These settings belong to Pinboard; MCP cannot modify Claude/Codex's native organization controls.

## Local validation

`node scripts/identity-smoke.mjs` launches disposable OIDC/SAML/SMTP fixtures and a local HTTPS proxy. It generates temporary keys, verifies signed AuthnRequests and PKCE exchanges, tests positive sign-ins and negative claim/signature/correlation cases, and exercises shared accounts, roles, invitations, comments, token revocation, and organization policy. The fixture trusts its own local HTTPS certificate only in its isolated browser contexts. Real provider configuration still needs a deployment-specific sign-in check.
