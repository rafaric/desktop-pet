import express from "express";
import cors from "cors";
import { readFileSync, writeFileSync, existsSync, mkdirSync } from "fs";
import { join, dirname, isAbsolute } from "path";
import { fileURLToPath } from "url";
import { spawn } from "child_process";

const __dirname = dirname(fileURLToPath(import.meta.url));
const PROJECT_ROOT = join(__dirname, "..", "..");
const DATA_DIR = join(__dirname, "data");
const DOWNLOADS_DIR = join(DATA_DIR, "downloads");

const generatorBinaryName =
	process.platform === "win32" ? "generate_petpack.exe" : "generate_petpack";

function resolveGeneratorBinary() {
	if (process.env.PETPACK_GENERATOR_BIN) {
		return process.env.PETPACK_GENERATOR_BIN;
	}

	const candidates = [
		join(PROJECT_ROOT, "src-tauri", "target", "debug", generatorBinaryName),
		join(PROJECT_ROOT, "src-tauri", "target", "release", generatorBinaryName),
	];

	return candidates.find((candidate) => existsSync(candidate)) ?? candidates[0];
}

function resolvePetSourceDir(sourceDir) {
	if (!sourceDir) return null;
	return isAbsolute(sourceDir) ? sourceDir : join(PROJECT_ROOT, sourceDir);
}

// Ensure directories exist
if (!existsSync(DOWNLOADS_DIR)) mkdirSync(DOWNLOADS_DIR, { recursive: true });

const readJson = (filename) =>
	JSON.parse(readFileSync(join(DATA_DIR, filename), "utf-8"));
const writeJson = (filename, data) =>
	writeFileSync(join(DATA_DIR, filename), JSON.stringify(data, null, 2));

const app = express();
app.use(cors());
app.use(express.json());

// ─── GET /pets ───────────────────────────────────────────────────────────────
app.get("/pets", (_req, res) => {
	const pets = readJson("pets.json");
	res.json({ pets });
});

// ─── GET /me/library ─────────────────────────────────────────────────────────
app.get("/me/library", (req, res) => {
	const accountId = req.headers["x-account-id"];
	if (!accountId) {
		return res.status(401).json({ error: "Missing x-account-id header" });
	}

	const entitlements = readJson("entitlements.json");
	const pets = readJson("pets.json");
	const owned = entitlements
		.filter((e) => e.accountId === accountId)
		.map((e) => {
			const pet = pets.find((p) => p.id === e.petId);
			if (!pet) return null;
			return {
				petId: pet.id,
				name: pet.name,
				description: pet.description,
				imageUrl: pet.imageUrl,
				purchasedAt: e.createdAt,
			};
		})
		.filter(Boolean);

	res.json({ pets: owned });
});

// ─── POST /entitlements ──────────────────────────────────────────────────────
app.post("/entitlements", (req, res) => {
	const { accountId, petId } = req.body;
	if (!accountId || !petId) {
		return res.status(400).json({ error: "accountId and petId are required" });
	}

	const pets = readJson("pets.json");
	if (!pets.find((p) => p.id === petId)) {
		return res.status(404).json({ error: "Pet not found" });
	}

	const entitlements = readJson("entitlements.json");
	const existing = entitlements.find(
		(e) => e.accountId === accountId && e.petId === petId,
	);
	if (existing) {
		return res
			.status(409)
			.json({ error: "Already owned", entitlement: existing });
	}

	const entitlement = {
		id: `ent_${Date.now()}`,
		accountId,
		petId,
		createdAt: new Date().toISOString(),
	};
	entitlements.push(entitlement);
	writeJson("entitlements.json", entitlements);

	res.status(201).json({ entitlement });
});

// ─── POST /downloads/pets/:petId ────────────────────────────────────────────
app.post("/downloads/pets/:petId", async (req, res) => {
	const accountId = req.headers["x-account-id"];
	if (!accountId) {
		return res.status(401).json({ error: "Missing x-account-id header" });
	}

	const { petId } = req.params;
	const pets = readJson("pets.json");
	const pet = pets.find((p) => p.id === petId);
	if (!pet) {
		return res.status(404).json({ error: "Pet not found" });
	}

	const entitlements = readJson("entitlements.json");
	const entitlement = entitlements.find(
		(e) => e.accountId === accountId && e.petId === petId,
	);
	if (!entitlement) {
		return res
			.status(403)
			.json({ error: "You do not own this pet. Please purchase it first." });
	}

	const sourceDir = resolvePetSourceDir(pet.sourceDir);
	if (!sourceDir) {
		return res
			.status(500)
			.json({ error: "Pet source not configured on server" });
	}
	if (!existsSync(sourceDir)) {
		return res.status(500).json({
			error: `Pet source directory not found: ${sourceDir}`,
		});
	}

	const outputFile = `${petId}-${accountId}.petpack`;
	const outputPath = join(DOWNLOADS_DIR, outputFile);
	const generatorBin = resolveGeneratorBinary();

	if (!existsSync(generatorBin)) {
		return res.status(500).json({
			error:
				"Generator binary not found. Run: cargo build --manifest-path src-tauri/Cargo.toml --bin generate_petpack",
		});
	}

	const proc = spawn(generatorBin, [
		sourceDir,
		outputPath,
		accountId,
		entitlement.id,
		entitlement.id,
	]);

	let errorOutput = "";
	proc.stderr.on("data", (data) => {
		errorOutput += data.toString();
	});

	const exitCode = await new Promise((resolve) => {
		proc.on("close", (code) => resolve(code ?? 1));
		proc.on("error", () => resolve(1));
	});

	if (exitCode !== 0 || !existsSync(outputPath)) {
		console.error("Petpack generation failed:", errorOutput);
		return res.status(500).json({ error: "Failed to generate petpack" });
	}

	res.json({
		downloadId: outputFile.replace(".petpack", ""),
		petId,
		status: "ready",
		downloadUrl: `http://localhost:3001/downloads/${outputFile.replace(".petpack", "")}`,
	});
});

// ─── GET /downloads/:downloadId ──────────────────────────────────────────────
app.get("/downloads/:downloadId", (req, res) => {
	const { downloadId } = req.params;
	const filePath = join(DOWNLOADS_DIR, `${downloadId}.petpack`);
	if (!existsSync(filePath)) {
		return res.status(404).json({ error: "Download not found or expired" });
	}
	res.download(filePath, `${downloadId}.petpack`);
});

// ─── Health ──────────────────────────────────────────────────────────────────
app.get("/health", (_req, res) => res.json({ status: "ok" }));

const PORT = process.env.DESKTOP_PET_BACKEND_PORT || process.env.PORT || 3001;
app.listen(PORT, () => {
	console.log(`Desktop Pet store backend running on http://localhost:${PORT}`);
});
