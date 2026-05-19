import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./App.css";

type PetPosition = "top-left" | "top-right" | "bottom-left" | "bottom-right";
type PetSize = "small" | "medium" | "large";

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

	return (
		<main className="main-shell">
			<section className="hero-card">
				<p className="eyebrow">PoC 1</p>
				<h1>Desktop Pet Overlay</h1>
				<p className="summary">
					Validamos la ventana transparente always-on-top, controles básicos y
					menú rápido antes de sumar puntos, skins o backend.
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

			<p className="status-line">{status}</p>
		</main>
	);
}

function DemoPet({ opacity, size }: { opacity: number; size: PetSize }) {
	return (
		<div className={`pet-stage ${size}`} style={{ opacity }}>
			<div className="pet-shadow" />
			<div className="pet-body">
				<div className="ear left" />
				<div className="ear right" />
				<div className="face">
					<div className="eye left" />
					<div className="eye right" />
					<div className="mouth" />
				</div>
				<div className="belly" />
			</div>
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

	useEffect(() => {
		const unlistenOpacity = listen<number>("pet-opacity-changed", (event) => {
			setOpacity(event.payload);
		});
		const unlistenSize = listen<PetSize>("pet-size-changed", (event) => {
			setSize(event.payload);
		});

		return () => {
			void unlistenOpacity.then((unlisten) => unlisten());
			void unlistenSize.then((unlisten) => unlisten());
		};
	}, []);

	return (
		<main className="overlay-shell">
			<DemoPet opacity={opacity} size={size} />
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
