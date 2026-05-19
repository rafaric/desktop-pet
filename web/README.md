# Desktop Pet web skeleton

This folder contains the first commercial web surface for Desktop Pet Companion.

## Purpose

The web app is responsible for:

- marketing the desktop app;
- linking Windows/macOS downloads;
- showing the pet catalog/store;
- requiring login before premium pet downloads;
- eventually generating account-bound signed petpacks through the backend.

The desktop app remains the runtime and validator. It should not become the primary commerce surface.

## Current status

This is a static skeleton only:

- no auth provider;
- no payment provider;
- no backend API;
- no real installer downloads;
- no real petpack generation.

## Local development

From the repository root:

```bash
npm run web:dev
```

Build the static site:

```bash
npm run web:build
```

Output goes to:

```text
dist-web/
```

## API contract

The first provider-neutral API draft lives at:

```text
services/store-api/openapi.yaml
```

It defines catalog, app downloads, owned pets, checkout, payment webhook, and licensed petpack download endpoints.

## Next implementation steps

1. Replace placeholder download links with real signed/notarized installers.
2. Add provider-neutral auth integration.
3. Add catalog data from the store API.
4. Add entitlement checks for owned pets.
5. Add licensed petpack download endpoint integration.
