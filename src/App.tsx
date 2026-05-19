import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
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

type ActivityEventPayload = {
	activity_kind: ActivityKind;
	stats: ActivityStats;
};

const initialActivityStats: ActivityStats = {
	points: 0,
	mouse_clicks: 0,
	key_presses: 0,
	tracking_enabled: true,
	pet_active: false,
};

const petIdleFrame = "/pets/demo/idle.png";
const petActiveFrames = [
	"/pets/demo/active-01.png",
	"/pets/demo/active-02.png",
	"/pets/demo/active-03.png",
	"/pets/demo/active-04.png",
	"/pets/demo/active-05.png",
];

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

	const [position, setPosition] = useState<PetPosition>("bottom-right");
	const [size, setSize] = useState<PetSize>("medium");
	const [opacity, setOpacity] = useState(1);
	const [status, setStatus] = useState("Ready to test the pet overlay.");
	const [activityStats, setActivityStats] =
		useState<ActivityStats>(initialActivityStats);
	const [lastActivity, setLastActivity] = useState<ActivityKind | null>(null);

	useEffect(() => {
		void invoke<ActivityStats>("get_activity_stats").then(setActivityStats);

		const unlistenStats = listen<ActivityStats>(
			"activity-stats-updated",
			(event) => setActivityStats(event.payload),
		);
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

	return (
		<main className="main-shell">
			<section className="hero-card">
				<p className="eyebrow">PoC 2</p>
				<h1>Desktop Pet Activity</h1>
				<p className="summary">
					Validamos puntos locales por clics y teclado globales solo mientras la
					mascota está activa. La app cuenta eventos, no guarda texto ni teclas
					exactas.
				</p>

				<div className="primary-actions">
					<button type="button" onClick={showConfiguredPet}>
						Mostrar mascota
					</button>
					<button
						className="secondary"
						type="button"
						onClick={() => runSafely(() => runCommand("hide_pet"))}
					>
						Ocultar mascota
					</button>
				</div>
			</section>

			<section className="stats-grid">
				<div className="stat-card highlight">
					<span>Puntos</span>
					<strong>{activityStats.points}</strong>
				</div>
				<div className="stat-card">
					<span>Clics</span>
					<strong>{activityStats.mouse_clicks}</strong>
				</div>
				<div className="stat-card">
					<span>Teclas</span>
					<strong>{activityStats.key_presses}</strong>
				</div>
				<div className="stat-card">
					<span>Última actividad</span>
					<strong>
						{lastActivity === "keyboard"
							? "Teclado"
							: lastActivity === "mouse"
								? "Ratón"
								: "—"}
					</strong>
				</div>
			</section>

			<section className="control-grid">
				<div className="control-card">
					<h2>Posición</h2>
					<div className="button-grid two-columns">
						<ControlButton
							active={position === "top-left"}
							onClick={() => updatePosition("top-left")}
						>
							Arriba izquierda
						</ControlButton>
						<ControlButton
							active={position === "top-right"}
							onClick={() => updatePosition("top-right")}
						>
							Arriba derecha
						</ControlButton>
						<ControlButton
							active={position === "bottom-left"}
							onClick={() => updatePosition("bottom-left")}
						>
							Abajo izquierda
						</ControlButton>
						<ControlButton
							active={position === "bottom-right"}
							onClick={() => updatePosition("bottom-right")}
						>
							Abajo derecha
						</ControlButton>
					</div>
				</div>

				<div className="control-card">
					<h2>Tamaño</h2>
					<div className="button-grid">
						{(["small", "medium", "large"] as PetSize[]).map((value) => (
							<ControlButton
								key={value}
								active={size === value}
								onClick={() => updateSize(value)}
							>
								{value === "small"
									? "Pequeño"
									: value === "medium"
										? "Mediano"
										: "Grande"}
							</ControlButton>
						))}
					</div>
				</div>

				<div className="control-card">
					<h2>Opacidad</h2>
					<div className="button-grid">
						{[1, 0.75, 0.5].map((value) => (
							<ControlButton
								key={value}
								active={opacity === value}
								onClick={() => updateOpacity(value)}
							>
								{Math.round(value * 100)}%
							</ControlButton>
						))}
					</div>
				</div>
			</section>

			<section className="privacy-card">
				<div>
					<h2>Privacidad de la PoC</h2>
					<p>
						Solo se guardan contadores numéricos locales. No guardamos texto,
						teclas exactas, posición del cursor ni aplicaciones usadas.
					</p>
					<p>
						Estado: mascota {activityStats.pet_active ? "activa" : "oculta"} ·
						tracking {activityStats.tracking_enabled ? "activo" : "pausado"}
					</p>
				</div>
				<button
					className={activityStats.tracking_enabled ? "secondary" : undefined}
					type="button"
					onClick={() => updateTracking(!activityStats.tracking_enabled)}
				>
					{activityStats.tracking_enabled
						? "Pausar tracking"
						: "Activar tracking"}
				</button>
			</section>

			<p className="status-line">{status}</p>
		</main>
	);
}

function DemoPet({
	opacity,
	size,
	active,
}: {
	opacity: number;
	size: PetSize;
	active: boolean;
}) {
	const [frameIndex, setFrameIndex] = useState(0);

	useEffect(() => {
		if (!active) {
			setFrameIndex(0);
			return;
		}

		const interval = window.setInterval(() => {
			setFrameIndex((current) => (current + 1) % petActiveFrames.length);
		}, 180);

		return () => window.clearInterval(interval);
	}, [active]);

	const frame = active ? petActiveFrames[frameIndex] : petIdleFrame;

	return (
		<div
			className={`pet-stage image-pet ${size} ${active ? "is-active" : ""}`}
			style={{ opacity }}
		>
			<img src={frame} alt="Demo desktop pet" draggable={false} />
		</div>
	);
}

function PetOverlay() {
	useEffect(() => {
		document.body.classList.add("pet-window");
		return () => document.body.classList.remove("pet-window");
	}, []);

	const [opacity, setOpacity] = useState(1);
	const [size, setSize] = useState<PetSize>("medium");
	const [active, setActive] = useState(false);
	const activeTimeout = useRef<number | null>(null);

	useEffect(() => {
		const unlistenOpacity = listen<number>("pet-opacity-changed", (event) => {
			setOpacity(event.payload);
		});
		const unlistenSize = listen<PetSize>("pet-size-changed", (event) => {
			setSize(event.payload);
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
			void unlistenActivity.then((unlisten) => unlisten());
		};
	}, []);

	return (
		<main className="overlay-shell">
			<DemoPet opacity={opacity} size={size} active={active} />
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
