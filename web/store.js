// store.js — minimal web store connecting to backend API
const API_BASE = "http://localhost:3001";

// ─── Account linking ─────────────────────────────────────────────────────────

function getLinkedAccountId() {
	return localStorage.getItem("linkedAccountId");
}

async function linkAccount(accountId) {
	if (!accountId || !accountId.trim()) {
		alert("Ingresá un ID de cuenta válido.");
		return;
	}
	localStorage.setItem("linkedAccountId", accountId.trim());
	await refreshLibrary();
	await refreshCatalog();
	renderStoreApp();
}

function unlinkAccount() {
	localStorage.removeItem("linkedAccountId");
	renderStoreApp();
	refreshCatalog();
}

// ─── API calls ───────────────────────────────────────────────────────────────

async function refreshCatalog() {
	try {
		const res = await fetch(`${API_BASE}/pets`);
		if (!res.ok) return;
		const data = await res.json();
		renderPetGrid(data.pets);
	} catch {
		// backend not running — show empty
	}
}

async function refreshLibrary() {
	const accountId = getLinkedAccountId();
	if (!accountId) return;
	try {
		const res = await fetch(`${API_BASE}/me/library`, {
			headers: { "x-account-id": accountId },
		});
		if (!res.ok) return;
		const data = await res.json();
		renderOwnedPets(data.pets);
	} catch {
		// backend not running
	}
}

async function grantEntitlement(petId) {
	const accountId = getLinkedAccountId();
	if (!accountId) {
		alert("Vinculá tu cuenta primero.");
		return;
	}
	const res = await fetch(`${API_BASE}/entitlements`, {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ accountId, petId }),
	});
	if (res.status === 409) {
		alert("Ya tenés esta mascota. Descargala abajo.");
	} else if (res.ok) {
		await refreshLibrary();
		alert("¡Mascota adquirida! Ahora descargala.");
	} else {
		const err = await res.json();
		alert(`Error: ${err.error}`);
	}
}

async function requestDownload(petId) {
	const accountId = getLinkedAccountId();
	if (!accountId) {
		alert("Vinculá tu cuenta primero.");
		return;
	}
	const res = await fetch(`${API_BASE}/downloads/pets/${petId}`, {
		method: "POST",
		headers: { "x-account-id": accountId },
	});
	if (!res.ok) {
		const err = await res.json();
		alert(`No se pudo generar: ${err.error}`);
		return;
	}
	const data = await res.json();
	window.location.href = data.downloadUrl;
}

// ─── Rendering ───────────────────────────────────────────────────────────────

function renderStoreApp() {
	const app = document.getElementById("store-app");
	if (!app) return;
	const linked = getLinkedAccountId();
	if (!linked) {
		app.innerHTML = `
      <div class="store-auth">
        <h3>Vinculá tu cuenta</h3>
        <p>Ingresá el ID de tu cuenta de Google (sub) que viste en la app de escritorio para comprar y descargar mascotas.</p>
        <div class="account-link-form">
          <input type="text" id="account-id-input" placeholder="ID de cuenta (ej: 1000220746...)" />
          <button class="button" onclick="linkAccount(document.getElementById('account-id-input').value)">
            Vincular
          </button>
        </div>
      </div>
    `;
	} else {
		app.innerHTML = `
      <div class="store-linked">
        <p>Cuenta vinculada: <code>${linked}</code>
          <button class="button small secondary" onclick="unlinkAccount()">Desvincular</button>
        </p>
      </div>
    `;
	}
}

function getPetEmoji(petId) {
	const map = { chimmy: "🐶", kero: "🦊", fox: "🦊", demo: "🐾" };
	return map[petId] || "🐾";
}

function renderPetGrid(pets) {
	const container = document.getElementById("pet-grid");
	if (!container) return;
	container.innerHTML = pets
		.map(
			(pet) => `
    <article class="pet-card">
      <div class="pet-art">${getPetEmoji(pet.id)}</div>
      <h3>${pet.name}</h3>
      <p>${pet.description}</p>
      <div class="pet-meta">
        <span>${pet.price === 0 ? "Gratis" : `USD ${(pet.price / 100).toFixed(2)}`}</span>
      </div>
      <button class="button small" onclick="grantEntitlement('${pet.id}')">
        Obtener
      </button>
    </article>
  `,
		)
		.join("");
}

function renderOwnedPets(pets) {
	const container = document.getElementById("owned-grid");
	if (!container) return;
	if (!pets || !pets.length) {
		container.innerHTML =
			'<p class="muted">No tenés mascotas compradas aún.</p>';
		return;
	}
	container.innerHTML = pets
		.map(
			(pet) => `
    <article class="pet-card owned">
      <div class="pet-art">${getPetEmoji(pet.petId)}</div>
      <h3>${pet.name}</h3>
      <p>${pet.description}</p>
      <button class="button small primary" onclick="requestDownload('${pet.petId}')">
        Descargar .petpack
      </button>
    </article>
  `,
		)
		.join("");
}

// ─── Bootstrap ───────────────────────────────────────────────────────────────

function initStore() {
	renderStoreApp();
	refreshCatalog();
	refreshLibrary();
}

if (document.readyState === "loading") {
	document.addEventListener("DOMContentLoaded", initStore);
} else {
	initStore();
}
