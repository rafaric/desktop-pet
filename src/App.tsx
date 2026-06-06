import { useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./App.css";

type PetPosition = "top-left" | "top-right" | "bottom-left" | "bottom-right";
type PetSize = "small" | "medium" | "large";
type ActivityKind = "mouse" | "keyboard";

type ActivityStats = {
	points: number;
	mouse_clicks: number;
	key_presses: number;
	tracking_enabled: boolean;
	pet_active: boolean;
};

type PetSettings = {
	position: PetPosition;
	size: PetSize;
	opacity: number;
};

type SkinState = {
	active_skin_id: string;
	unlocked_skins: Record<string, string[]>;
};

type PetLibraryState = {
	active_pet_id: string;
	downloaded_pets: string[];
};

type AccountSession = {
	id: string;
	email: string;
	display_name: string;
	access_token?: string;
};

type PersistedState = {
	activity: ActivityStats;
	settings: PetSettings;
	skins: SkinState;
	pets: PetLibraryState;
	account: {
		account: AccountSession;
		id_token?: string;
	};
};

type ActivityEventPayload = {
	activity_kind: ActivityKind;
	stats: ActivityStats;
};

type PetSkinCatalogItem = {
	id: string;
	name: string;
	price: number;
	description: string;
};

type PetManifest = {
	id: string;
	name: string;
	status: string;
	description: string;
	previewFrame: string;
	idleFrame: string;
	activeFrames: string[];
	supportsSkins: boolean;
	skins: PetSkinCatalogItem[];
	source?: "bundled" | "imported";
};

type PetIndexFile = {
	pets: { id: string; manifest: string }[];
};

const initialActivityStats: ActivityStats = {
	points: 0,
	mouse_clicks: 0,
	key_presses: 0,
	tracking_enabled: true,
	pet_active: false,
};

const demoPetId = "demo";
const developmentAccountId = "user_test";
const petIdCollisionPrefix = "PET_ID_COLLISION:";
const devImportsEnabled =
	import.meta.env.DEV ||
	import.meta.env.VITE_DESKTOP_PET_DEV_IMPORTS === "true";
const googleClientId = import.meta.env.VITE_GOOGLE_CLIENT_ID?.trim();
const googleClientSecret = import.meta.env.VITE_GOOGLE_CLIENT_SECRET?.trim();

type ImportCollision = {
	kind: "folder" | "petpack";
	petId: string;
};

function getCollidingPetId(error: unknown) {
	const message = String(error);
	return message.startsWith(petIdCollisionPrefix)
		? message.slice(petIdCollisionPrefix.length)
		: null;
}

function friendlyImportError(error: unknown) {
	const message = String(error);

	if (message.includes("petpack license belongs to another account")) {
		return "Este petpack pertenece a otra cuenta.";
	}
	if (message.includes("petpack asset hash mismatch")) {
		return "Este petpack parece estar dañado o modificado.";
	}
	if (message.includes("petpack signature verification failed")) {
		return "No se pudo verificar la firma del petpack.";
	}
	if (message.includes("petpack signature is not valid base64")) {
		return "La firma del petpack tiene un formato inválido.";
	}
	if (message.includes("commercial petpack metadata is incomplete")) {
		return "Este petpack no tiene toda la metadata comercial requerida.";
	}
	if (
		message.includes("petpack metadata does not declare all runtime assets")
	) {
		return "Este petpack no declara todos los assets que usa la mascota.";
	}
	if (message.includes("petpack contains an unsafe path")) {
		return "Este petpack contiene rutas inseguras.";
	}
	if (message.includes("petpack contains too many files")) {
		return "Este petpack contiene demasiados archivos.";
	}
	if (message.includes("petpack contains a file that is too large")) {
		return "Este petpack contiene un archivo demasiado grande.";
	}
	if (message.includes("petpack is too large")) {
		return "Este petpack es demasiado grande.";
	}

	return message;
}

const defaultPetCatalog: PetManifest[] = [
	{
		id: demoPetId,
		name: "Demo Pet",
		status: "Descargada",
		description: "Mascota local de prueba incluida en el prototipo.",
		previewFrame: "/pets/demo/idle.png",
		idleFrame: "/pets/demo/idle.png",
		activeFrames: [
			"/pets/demo/active-01.png",
			"/pets/demo/active-02.png",
			"/pets/demo/active-03.png",
			"/pets/demo/active-04.png",
			"/pets/demo/active-05.png",
		],
		supportsSkins: true,
		source: "bundled",
		skins: [
			{
				id: "default",
				name: "Original",
				price: 0,
				description: "Aspecto base de la mascota demo.",
			},
			{
				id: "mint",
				name: "Menta",
				price: 25,
				description: "Tono frío y suave.",
			},
			{
				id: "berry",
				name: "Berry",
				price: 50,
				description: "Variante rosada más intensa.",
			},
			{
				id: "night",
				name: "Noche",
				price: 100,
				description: "Look oscuro para escritorios nocturnos.",
			},
		],
	},
	{
		id: "fox",
		name: "Fox",
		status: "Próximamente",
		description: "Ejemplo de futura mascota comprada y descargada.",
		previewFrame: "/pets/demo/idle.png",
		idleFrame: "/pets/demo/idle.png",
		activeFrames: ["/pets/demo/active-01.png"],
		supportsSkins: false,
		source: "bundled",
		skins: [],
	},
];

const initialSkinState: SkinState = {
	active_skin_id: "default",
	unlocked_skins: { [demoPetId]: ["default"] },
};

const initialPetLibraryState: PetLibraryState = {
	active_pet_id: demoPetId,
	downloaded_pets: [demoPetId],
};

const initialAccountSession: AccountSession = {
	id: developmentAccountId,
	email: "user_test@example.com",
	display_name: "Development User",
};

function resolveManifestAsset(manifestUrl: string, assetPath: string) {
	if (assetPath.startsWith("/")) {
		return assetPath;
	}

	return new URL(
		assetPath,
		new URL(manifestUrl, window.location.origin),
	).toString();
}

function hydrateImportedPetAssets(pet: PetManifest): PetManifest {
	return {
		...pet,
		previewFrame: convertFileSrc(pet.previewFrame),
		idleFrame: convertFileSrc(pet.idleFrame),
		activeFrames: pet.activeFrames.map((frame) => convertFileSrc(frame)),
		source: "imported",
	};
}

async function loadBundledPetCatalog(): Promise<PetManifest[]> {
	const indexResponse = await fetch("/pets/index.json");
	if (!indexResponse.ok) {
		throw new Error("pet index not found");
	}

	const index = (await indexResponse.json()) as PetIndexFile;
	const manifestResults = await Promise.allSettled(
		index.pets.map(async (pet) => {
			const manifestResponse = await fetch(pet.manifest);
			if (!manifestResponse.ok) {
				throw new Error(`manifest not found for ${pet.id}`);
			}

			const manifest = (await manifestResponse.json()) as PetManifest;
			return {
				...manifest,
				source: "bundled" as const,
				previewFrame: resolveManifestAsset(pet.manifest, manifest.previewFrame),
				idleFrame: resolveManifestAsset(pet.manifest, manifest.idleFrame),
				activeFrames: manifest.activeFrames.map((frame) =>
					resolveManifestAsset(pet.manifest, frame),
				),
			};
		}),
	);

	return manifestResults.reduce<PetManifest[]>((catalog, result) => {
		if (result.status === "fulfilled") {
			catalog.push(result.value);
		}
		return catalog;
	}, []);
}

async function loadLocalPetCatalog(): Promise<PetManifest[]> {
	try {
		const [bundledPets, installedPets] = await Promise.all([
			loadBundledPetCatalog().catch(() => defaultPetCatalog),
			invoke<PetManifest[]>("get_installed_pet_catalog").catch(() => []),
		]);

		const mergedCatalog = [
			...bundledPets,
			...installedPets.map((pet) => hydrateImportedPetAssets(pet)),
		];

		return mergedCatalog.length > 0 ? mergedCatalog : defaultPetCatalog;
	} catch {
		return defaultPetCatalog;
	}
}

function normalisePetFrames(pet: PetManifest) {
	return pet.activeFrames.length > 0 ? pet.activeFrames : [pet.idleFrame];
}

type ControlButtonProps = {
	children: React.ReactNode;
	onClick: () => void;
	active?: boolean;
};

function ControlButton({
	children,
	onClick,
	active = false,
}: ControlButtonProps) {
	return (
		<button
			className={active ? "active" : undefined}
			type="button"
			onClick={onClick}
		>
			{children}
		</button>
	);
}

function MainWindow() {
	useEffect(() => {
		document.body.classList.add("main-window");
		return () => document.body.classList.remove("main-window");
	}, []);

	const [petCatalog, setPetCatalog] =
		useState<PetManifest[]>(defaultPetCatalog);
	const [importPath, setImportPath] = useState("");
	const [petpackPath, setPetpackPath] = useState("");
	const [position, setPosition] = useState<PetPosition>("bottom-right");
	const [size, setSize] = useState<PetSize>("medium");
	const [opacity, setOpacity] = useState(1);
	const [status, setStatus] = useState("Ready to test the pet overlay.");
	const [importFeedback, setImportFeedback] = useState<{
		kind: "error" | "success";
		message: string;
	} | null>(null);
	const [importCollision, setImportCollision] =
		useState<ImportCollision | null>(null);
	const [activityStats, setActivityStats] =
		useState<ActivityStats>(initialActivityStats);
	const [skinState, setSkinState] = useState<SkinState>(initialSkinState);
	const [petLibrary, setPetLibrary] = useState<PetLibraryState>(
		initialPetLibraryState,
	);
	const [currentAccount, setCurrentAccount] = useState<AccountSession>(
		initialAccountSession,
	);
	const [lastActivity, setLastActivity] = useState<ActivityKind | null>(null);
	const [activeTab, setActiveTab] = useState<
		"pet" | "skins" | "stats" | "settings"
	>("pet");

	async function refreshPetCatalog() {
		setPetCatalog(await loadLocalPetCatalog());
	}

	useEffect(() => {
		void refreshPetCatalog();
	}, []);

	useEffect(() => {
		void invoke<AccountSession>("get_current_account").then(setCurrentAccount);
	}, []);

	useEffect(() => {
		void invoke<PersistedState>("get_app_state").then((persisted) => {
			setActivityStats(persisted.activity);
			setSkinState(persisted.skins);
			setPetLibrary(persisted.pets);
			setPosition(persisted.settings.position);
			setSize(persisted.settings.size);
			setOpacity(persisted.settings.opacity);
			setCurrentAccount(persisted.account.account);
		});

		const unlistenStats = listen<ActivityStats>(
			"activity-stats-updated",
			(event) => setActivityStats(event.payload),
		);
		const unlistenSettings = listen<PetSettings>(
			"pet-settings-updated",
			(event) => {
				setPosition(event.payload.position);
				setSize(event.payload.size);
				setOpacity(event.payload.opacity);
			},
		);
		const unlistenSkinState = listen<SkinState>("skin-state-updated", (event) =>
			setSkinState(event.payload),
		);
		const unlistenPetLibrary = listen<PetLibraryState>(
			"pet-library-updated",
			(event) => setPetLibrary(event.payload),
		);
		const unlistenPetCatalog = listen<boolean>("pet-catalog-changed", () => {
			void refreshPetCatalog();
		});
		const unlistenActivity = listen<ActivityEventPayload>(
			"activity-detected",
			(event) => {
				setActivityStats(event.payload.stats);
				setLastActivity(event.payload.activity_kind);
				window.setTimeout(() => setLastActivity(null), 900);
			},
		);
		const unlistenError = listen<string>("activity-listener-error", (event) => {
			setStatus(`Activity listener failed: ${event.payload}`);
		});

		return () => {
			void unlistenStats.then((unlisten) => unlisten());
			void unlistenSettings.then((unlisten) => unlisten());
			void unlistenSkinState.then((unlisten) => unlisten());
			void unlistenPetLibrary.then((unlisten) => unlisten());
			void unlistenPetCatalog.then((unlisten) => unlisten());
			void unlistenActivity.then((unlisten) => unlisten());
			void unlistenError.then((unlisten) => unlisten());
		};
	}, []);

	async function runCommand(command: string, args?: Record<string, unknown>) {
		await invoke(command, args);
		setStatus("Command applied successfully.");
	}

	async function runSafely(action: () => Promise<void>) {
		try {
			await action();
		} catch (error) {
			setStatus(`Command failed: ${String(error)}`);
		}
	}

	async function showConfiguredPet() {
		await runSafely(async () => {
			await runCommand("show_pet");
			await runCommand("set_pet_size", { size });
			await runCommand("set_pet_position", { position });
			await runCommand("set_pet_opacity", { opacity });
		});
	}

	async function updatePosition(nextPosition: PetPosition) {
		await runSafely(async () => {
			await runCommand("set_pet_position", { position: nextPosition });
			setPosition(nextPosition);
		});
	}

	async function updateSize(nextSize: PetSize) {
		await runSafely(async () => {
			await runCommand("set_pet_size", { size: nextSize });
			await runCommand("set_pet_position", { position });
			setSize(nextSize);
		});
	}

	async function updateOpacity(nextOpacity: number) {
		await runSafely(async () => {
			await runCommand("set_pet_opacity", { opacity: nextOpacity });
			setOpacity(nextOpacity);
		});
	}

	async function updateTracking(enabled: boolean) {
		await runSafely(async () => {
			const stats = await invoke<ActivityStats>(
				"set_activity_tracking_enabled",
				{ enabled },
			);
			setActivityStats(stats);
			setStatus(
				enabled ? "Activity tracking enabled." : "Activity tracking paused.",
			);
		});
	}

	async function handleGoogleLogin() {
		setStatus("Connecting with Google...");
		setImportFeedback(null);

		if (!googleClientId || !googleClientSecret) {
			setStatus("Google OAuth is not configured.");
			setImportFeedback({
				kind: "error",
				message:
					"Falta configurar Google OAuth en el entorno local. Copiá las variables desde env.example.txt a tu .env.",
			});
			return;
		}

		try {
			const { signIn } = await import(
				"@choochmeque/tauri-plugin-google-auth-api"
			);
			const response = await signIn({
				clientId: googleClientId,
				clientSecret: googleClientSecret,
				scopes: ["openid", "email", "profile"],
			});

			// Decode ID token to get user info
			let userInfo = { sub: "", email: "", name: "" };
			if (response.idToken) {
				const payload = response.idToken.split(".")[1];
				const decoded = JSON.parse(
					atob(payload.replace(/-/g, "+").replace(/_/g, "/")),
				);
				userInfo = {
					sub: decoded.sub ?? "",
					email: decoded.email ?? "",
					name: decoded.name ?? "",
				};
			}

			const account: AccountSession = {
				id: userInfo.sub || developmentAccountId,
				email: userInfo.email || developmentAccountId,
				display_name: userInfo.name || "Google User",
				access_token: response.accessToken,
			};

			setCurrentAccount(account);
			await invoke("save_account_session", {
				account,
				idToken: response.idToken ?? null,
			});
			setStatus(`Logged in as ${account.display_name}`);
		} catch (error) {
			setStatus(`Login failed: ${error}`);
			setImportFeedback({
				kind: "error",
				message: `No se pudo iniciar sesión: ${String(error)}`,
			});
		}
	}

	async function handleGoogleLogout() {
		try {
			const { signOut } = await import(
				"@choochmeque/tauri-plugin-google-auth-api"
			);
			if (currentAccount.access_token) {
				await signOut({ accessToken: currentAccount.access_token });
			} else {
				await signOut();
			}
		} catch {
			// ignore signOut errors
		}
		setCurrentAccount(initialAccountSession);
		await invoke("clear_account_session");
		setStatus("Logged out.");
	}

	const activePetId = petLibrary.active_pet_id;
	const activePet =
		petCatalog.find((pet) => pet.id === activePetId) ??
		petCatalog.find((pet) => pet.id === demoPetId) ??
		defaultPetCatalog[0];
	const activePetSupportsSkins =
		activePet.id === demoPetId && activePet.supportsSkins;
	const activePetSkins = activePetSupportsSkins ? activePet.skins : [];

	function isSkinUnlocked(skinId: string) {
		return skinState.unlocked_skins[activePetId]?.includes(skinId) ?? false;
	}

	async function usePet(petId: string) {
		await runSafely(async () => {
			const pets = await invoke<PetLibraryState>("set_active_pet", { petId });
			setPetLibrary(pets);
			setStatus("Pet selected.");
		});
	}

	async function importPetFolder(overwriteExisting = false) {
		setImportFeedback(null);
		if (overwriteExisting) {
			setImportCollision(null);
		}

		try {
			if (!importPath.trim()) {
				throw new Error("enter a local pet folder path first");
			}

			const persisted = await invoke<PersistedState>("import_pet_from_folder", {
				folderPath: importPath,
				overwriteExisting,
			});
			setImportCollision(null);
			setPetLibrary(persisted.pets);
			await refreshPetCatalog();
			setStatus("Pet imported successfully.");
			setImportFeedback({
				kind: "success",
				message: overwriteExisting
					? "Mascota reemplazada correctamente."
					: "Mascota importada correctamente.",
			});
		} catch (error) {
			const collidingPetId = getCollidingPetId(error);
			if (collidingPetId) {
				setImportCollision({ kind: "folder", petId: collidingPetId });
				setImportFeedback(null);
				return;
			}

			const message = friendlyImportError(error);
			setStatus(`Command failed: ${String(error)}`);
			setImportFeedback({
				kind: "error",
				message: `No se pudo importar: ${message}`,
			});
		}
	}

	async function pickPetFolder() {
		setImportFeedback(null);
		const selected = await open({
			directory: true,
			multiple: false,
			title: "Seleccioná una carpeta de mascota",
		});

		if (typeof selected === "string") {
			setImportPath(selected);
			setStatus("Pet folder selected.");
		}
	}

	async function importPetpackFile(overwriteExisting = false) {
		setImportFeedback(null);
		if (overwriteExisting) {
			setImportCollision(null);
		}

		try {
			if (!petpackPath.trim()) {
				throw new Error("select a .petpack or .zip file first");
			}

			const persisted = await invoke<PersistedState>("import_petpack_file", {
				filePath: petpackPath,
				overwriteExisting,
			});
			setImportCollision(null);
			setPetLibrary(persisted.pets);
			await refreshPetCatalog();
			setStatus("Petpack imported successfully.");
			setImportFeedback({
				kind: "success",
				message: overwriteExisting
					? "Petpack reemplazado correctamente."
					: "Petpack importado correctamente.",
			});
		} catch (error) {
			const collidingPetId = getCollidingPetId(error);
			if (collidingPetId) {
				setImportCollision({ kind: "petpack", petId: collidingPetId });
				setImportFeedback(null);
				return;
			}

			const message = friendlyImportError(error);
			setStatus(`Command failed: ${String(error)}`);
			setImportFeedback({
				kind: "error",
				message: `No se pudo importar el petpack: ${message}`,
			});
		}
	}

	async function confirmImportReplacement() {
		if (!importCollision) {
			return;
		}

		if (importCollision.kind === "folder") {
			await importPetFolder(true);
			return;
		}

		await importPetpackFile(true);
	}

	function cancelImportReplacement() {
		setImportCollision(null);
		setImportFeedback({
			kind: "error",
			message: "Importación cancelada: la mascota ya estaba instalada.",
		});
	}

	async function pickPetpackFile() {
		setImportFeedback(null);
		const selected = await open({
			directory: false,
			multiple: false,
			title: "Seleccioná un .petpack o .zip",
			filters: [{ name: "Petpack", extensions: ["petpack", "zip"] }],
		});

		if (typeof selected === "string") {
			setPetpackPath(selected);
			setStatus("Petpack file selected.");
		}
	}

	async function unlockOrUseSkin(skin: PetSkinCatalogItem) {
		await runSafely(async () => {
			if (!activePetSupportsSkins) {
				throw new Error(
					"skins are only available for the active pet when its manifest enables them",
				);
			}

			if (isSkinUnlocked(skin.id)) {
				const skins = await invoke<SkinState>("set_active_skin", {
					petId: activePetId,
					skinId: skin.id,
				});
				setSkinState(skins);
				setStatus(`${skin.name} skin applied.`);
				return;
			}

			const persisted = await invoke<PersistedState>("unlock_skin", {
				petId: activePetId,
				skinId: skin.id,
			});
			setActivityStats(persisted.activity);
			setSkinState(persisted.skins);
			setStatus(`${skin.name} skin unlocked and applied.`);
		});
	}

	return (
		<main className="drawer-shell">
			<header className="drawer-header">
				<div className="drawer-pet-info">
					<img
						src={activePet.previewFrame}
						alt={activePet.name}
						className="drawer-pet-avatar"
					/>
					<div>
						<strong>{activePet.name}</strong>
						<span>{activityStats.points} puntos</span>
					</div>
				</div>
				<div className="drawer-header-actions">
					<button
						type="button"
						className="icon-btn"
						title="Mostrar"
						onClick={showConfiguredPet}
					>
						👁
					</button>
					<button
						type="button"
						className="icon-btn"
						title="Ocultar"
						onClick={() => runSafely(() => runCommand("hide_pet"))}
					>
						🫥
					</button>
				</div>
			</header>

			<nav className="drawer-tabs">
				{(["pet", "skins", "stats", "settings"] as const).map((tab) => (
					<button
						key={tab}
						type="button"
						className={`drawer-tab ${activeTab === tab ? "active" : ""}`}
						onClick={() => setActiveTab(tab)}
					>
						{tab === "pet"
							? "Mascotas"
							: tab === "skins"
								? "Estilos"
								: tab === "stats"
									? "Stats"
									: "Ajustes"}
					</button>
				))}
			</nav>

			<div className="drawer-content">
				{activeTab === "pet" && (
					<>
						<div className="pet-list">
							{petCatalog.map((pet) => {
								const downloaded = petLibrary.downloaded_pets.includes(pet.id);
								const active = petLibrary.active_pet_id === pet.id;
								return (
									<div
										key={pet.id}
										className={`pet-row ${active ? "active" : ""}`}
									>
										<img src={pet.previewFrame} alt={pet.name} />
										<div className="pet-row-info">
											<strong>{pet.name}</strong>
											<span>
												{active
													? "activa"
													: downloaded
														? "instalada"
														: pet.status}
											</span>
										</div>
										<button
											className={active ? "active" : "secondary"}
											disabled={active || !downloaded}
											type="button"
											onClick={() => usePet(pet.id)}
										>
											{active ? "✓" : downloaded ? "Usar" : "—"}
										</button>
									</div>
								);
							})}
						</div>
						{devImportsEnabled && (
							<div className="dev-import">
								<p className="eyebrow">Agregar mascota</p>
								<div className="import-row">
									<input
										type="text"
										value={importPath}
										onChange={(e) => setImportPath(e.currentTarget.value)}
										placeholder="Carpeta..."
									/>
									<button
										className="secondary"
										type="button"
										onClick={pickPetFolder}
									>
										📁
									</button>
									<button type="button" onClick={() => importPetFolder()}>
										Instalar
									</button>
								</div>
								<div className="import-row">
									<input
										type="text"
										value={petpackPath}
										onChange={(e) => setPetpackPath(e.currentTarget.value)}
										placeholder=".petpack..."
									/>
									<button
										className="secondary"
										type="button"
										onClick={pickPetpackFile}
									>
										📦
									</button>
									<button type="button" onClick={() => importPetpackFile()}>
										Instalar
									</button>
								</div>
								{importCollision && (
									<div className="import-collision">
										<div>
											<strong>"{importCollision.petId}" ya instalada.</strong>
											<p>¿Reemplazar?</p>
										</div>
										<div className="import-collision-actions">
											<button type="button" onClick={confirmImportReplacement}>
												Sí
											</button>
											<button
												className="secondary"
												type="button"
												onClick={cancelImportReplacement}
											>
												No
											</button>
										</div>
									</div>
								)}
								{importFeedback && (
									<p className={`import-feedback ${importFeedback.kind}`}>
										{importFeedback.message}
									</p>
								)}
							</div>
						)}
					</>
				)}

				{activeTab === "skins" && (
					<div className="skins-list">
						{!activePetSupportsSkins ? (
							<p className="drawer-empty">
								{activePet.name} no tiene estilos todavía.
							</p>
						) : (
							activePetSkins.map((skin) => {
								const unlocked = isSkinUnlocked(skin.id);
								const active = skinState.active_skin_id === skin.id;
								const affordable = activityStats.points >= skin.price;
								return (
									<div
										key={skin.id}
										className={`skin-row skin-${skin.id} ${active ? "active" : ""}`}
									>
										<img src={activePet.previewFrame} alt={skin.name} />
										<div className="skin-row-info">
											<strong>{skin.name}</strong>
											<span>{unlocked ? "✓" : `${skin.price} pts`}</span>
										</div>
										<button
											className={active ? "active" : "secondary"}
											disabled={active || (!unlocked && !affordable)}
											type="button"
											onClick={() => unlockOrUseSkin(skin)}
										>
											{active
												? "Usando"
												: unlocked
													? "Aplicar"
													: affordable
														? "Desbloquear"
														: "Faltan pts"}
										</button>
									</div>
								);
							})
						)}
					</div>
				)}

				{activeTab === "stats" && (
					<div className="stats-panel">
						<div className="stat-big">
							<span>Puntos</span>
							<strong>{activityStats.points}</strong>
						</div>
						<div className="stats-row">
							<div className="stat-item">
								<span>Clics</span>
								<strong>{activityStats.mouse_clicks}</strong>
							</div>
							<div className="stat-item">
								<span>Teclas</span>
								<strong>{activityStats.key_presses}</strong>
							</div>
							<div className="stat-item">
								<span>Última</span>
								<strong>
									{lastActivity === "keyboard"
										? "⌨️"
										: lastActivity === "mouse"
											? "🖱️"
											: "—"}
								</strong>
							</div>
						</div>
					</div>
				)}

				{activeTab === "settings" && (
					<div className="settings-panel">
						<div className="settings-group">
							<p className="settings-label">
								{currentAccount.id === developmentAccountId
									? "Sin sesión"
									: "Tu cuenta"}
							</p>
							<strong>{currentAccount.display_name}</strong>
							{currentAccount.id !== developmentAccountId && (
								<span className="settings-sub">{currentAccount.email}</span>
							)}
							{currentAccount.id === developmentAccountId ? (
								<button type="button" onClick={handleGoogleLogin}>
									Entrar con Google
								</button>
							) : (
								<button
									className="secondary"
									type="button"
									onClick={handleGoogleLogout}
								>
									Salir
								</button>
							)}
						</div>
						<div className="settings-group">
							<p className="settings-label">Posición</p>
							<div className="button-grid two-columns">
								{[
									["top-left", "↖ Arriba izq"],
									["top-right", "↗ Arriba der"],
									["bottom-left", "↙ Abajo izq"],
									["bottom-right", "↘ Abajo der"],
								].map(([val, label]) => (
									<ControlButton
										key={val}
										active={position === val}
										onClick={() => updatePosition(val as typeof position)}
									>
										{label}
									</ControlButton>
								))}
							</div>
						</div>
						<div className="settings-group">
							<p className="settings-label">Tamaño</p>
							<div className="button-grid">
								{(["small", "medium", "large"] as PetSize[]).map((val) => (
									<ControlButton
										key={val}
										active={size === val}
										onClick={() => updateSize(val)}
									>
										{val === "small"
											? "Pequeño"
											: val === "medium"
												? "Mediano"
												: "Grande"}
									</ControlButton>
								))}
							</div>
						</div>
						<div className="settings-group">
							<p className="settings-label">Opacidad</p>
							<div className="button-grid">
								{[1, 0.75, 0.5].map((val) => (
									<ControlButton
										key={val}
										active={opacity === val}
										onClick={() => updateOpacity(val)}
									>
										{Math.round(val * 100)}%
									</ControlButton>
								))}
							</div>
						</div>
						<div className="settings-group">
							<p className="settings-label">Privacidad</p>
							<span className="settings-sub">
								Solo contamos clics y teclas. Todo queda en tu computadora.
							</span>
							<button
								className={
									activityStats.tracking_enabled ? "secondary" : undefined
								}
								type="button"
								onClick={() => updateTracking(!activityStats.tracking_enabled)}
							>
								{activityStats.tracking_enabled
									? "Pausar actividad"
									: "Reanudar actividad"}
							</button>
						</div>
					</div>
				)}
			</div>

			<p className="status-line">{status}</p>
		</main>
	);
}

function PetSprite({
	opacity,
	size,
	active,
	skinId,
	pet,
}: {
	opacity: number;
	size: PetSize;
	active: boolean;
	skinId: string;
	pet: PetManifest;
}) {
	const [frameIndex, setFrameIndex] = useState(0);
	const activeFrames = normalisePetFrames(pet);

	useEffect(() => {
		if (!active) {
			setFrameIndex(0);
			return;
		}

		const interval = window.setInterval(() => {
			setFrameIndex((current) => (current + 1) % activeFrames.length);
		}, 180);

		return () => window.clearInterval(interval);
	}, [active, activeFrames.length]);

	const frame = active ? activeFrames[frameIndex] : pet.idleFrame;
	const renderedSkinId =
		pet.id === demoPetId && pet.supportsSkins ? skinId : "default";

	return (
		<div
			className={`pet-stage image-pet ${size} skin-${renderedSkinId} ${active ? "is-active" : ""}`}
			style={{ opacity }}
		>
			<img src={frame} alt={`${pet.name} desktop pet`} draggable={false} />
		</div>
	);
}

function PetOverlay() {
	useEffect(() => {
		document.body.classList.add("pet-window");
		return () => document.body.classList.remove("pet-window");
	}, []);

	const [petCatalog, setPetCatalog] =
		useState<PetManifest[]>(defaultPetCatalog);
	const [opacity, setOpacity] = useState(1);
	const [size, setSize] = useState<PetSize>("medium");
	const [skinId, setSkinId] = useState("default");
	const [petId, setPetId] = useState(demoPetId);
	const [active, setActive] = useState(false);
	const activeTimeout = useRef<number | null>(null);

	async function refreshPetCatalog() {
		setPetCatalog(await loadLocalPetCatalog());
	}

	useEffect(() => {
		void refreshPetCatalog();
	}, []);

	useEffect(() => {
		void invoke<PersistedState>("get_app_state").then((persisted) => {
			setOpacity(persisted.settings.opacity);
			setSize(persisted.settings.size);
			setSkinId(persisted.skins.active_skin_id);
			setPetId(persisted.pets.active_pet_id);
		});

		const unlistenOpacity = listen<number>("pet-opacity-changed", (event) => {
			setOpacity(event.payload);
		});
		const unlistenSize = listen<PetSize>("pet-size-changed", (event) => {
			setSize(event.payload);
		});
		const unlistenSkin = listen<string>("pet-skin-changed", (event) => {
			setSkinId(event.payload);
		});
		const unlistenPet = listen<string>("pet-active-changed", (event) => {
			setPetId(event.payload);
			if (!petCatalog.find((pet) => pet.id === event.payload)) {
				void refreshPetCatalog();
			}
		});
		const unlistenPetCatalog = listen<boolean>("pet-catalog-changed", () => {
			void refreshPetCatalog();
		});
		const unlistenActivity = listen<ActivityEventPayload>(
			"activity-detected",
			() => {
				setActive(true);
				if (activeTimeout.current !== null) {
					window.clearTimeout(activeTimeout.current);
				}
				activeTimeout.current = window.setTimeout(() => setActive(false), 900);
			},
		);

		return () => {
			if (activeTimeout.current !== null) {
				window.clearTimeout(activeTimeout.current);
			}
			void unlistenOpacity.then((unlisten) => unlisten());
			void unlistenSize.then((unlisten) => unlisten());
			void unlistenSkin.then((unlisten) => unlisten());
			void unlistenPet.then((unlisten) => unlisten());
			void unlistenPetCatalog.then((unlisten) => unlisten());
			void unlistenActivity.then((unlisten) => unlisten());
		};
	}, []);

	const activePet =
		petCatalog.find((pet) => pet.id === petId) ??
		petCatalog.find((pet) => pet.id === demoPetId) ??
		defaultPetCatalog[0];

	return (
		<main className="overlay-shell">
			<PetSprite
				opacity={opacity}
				size={size}
				active={active}
				skinId={skinId}
				pet={activePet}
			/>
		</main>
	);
}

function App() {
	const windowLabel = useMemo(() => getCurrentWindow().label, []);

	if (windowLabel === "pet-overlay") {
		return <PetOverlay />;
	}

	return <MainWindow />;
}

export default App;
