import { t, useTranslation } from "../i18n";
import { useEffect, useState, type ChangeEvent, type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
	Monitor,
	Eye,
	EyeOff,
	Circle,
	Layers,
	Columns2,
	Maximize2,
	Keyboard,
	Sparkles,
	RotateCcw,
	Timer,
	LayoutPanelLeft,
	HardDrive,
	Trash2,
	Copy,
	FolderOpen,
	FolderPlus,
	Plus,
	X
} from "lucide-react";
import { SettingRow } from "./SettingRow";
import { isMac } from "../platform";
import {
	KNOWN_FOLDER_OPTIONS,
	driveLetter,
	parseEnabledFolders,
	parseCustomFolders,
	parseHiddenDrives,
	type DockExtra
} from "../dockExtras";

/** Where a setting row's label starts: the row icon's 28px plus its 12px margin.
 * Content under a label (chips, lists) is indented by this to line up with it. */
const ROW_CONTENT_INDENT = "40px";

/** One chip per connected drive, plus hidden drives that aren't connected. */
function DrivePicker({
	hiddenDrives,
	onToggle
}: {
	hiddenDrives: string;
	onToggle: (letter: string) => void;
}) {
	const [drives, setDrives] = useState<{ letter: string; name: string }[]>([]);

	useEffect(() => {
		invoke<DockExtra[]>("get_dock_drives")
			.then((list) =>
				setDrives(
					list.flatMap((drive) => {
						const letter = driveLetter(drive);
						return letter ? [{ letter, name: drive.name }] : [];
					})
				)
			)
			.catch((error) => console.error("Failed to list drives:", error));
	}, []);

	const hidden = parseHiddenDrives(hiddenDrives);
	const disconnected = hidden
		.filter((letter) => !drives.some((drive) => drive.letter === letter))
		.map((letter) => ({ letter, name: `${letter}: (not connected)` }));

	return (
		<div style={{ display: "flex", gap: "8px", flexWrap: "wrap", paddingLeft: ROW_CONTENT_INDENT }}>
			{[...drives, ...disconnected].map((drive) => (
				<button
					key={drive.letter}
					type="button"
					className={`folder-chip ${hidden.includes(drive.letter) ? "" : "active"}`}
					onClick={() => onToggle(drive.letter)}
				>
					{drive.name}
				</button>
			))}
		</div>
	);
}

const START_ICON_PRESETS = [
	{ key: "default", src: "/bloom.png", labelKey: "settings.dock.iconBloom" },
	{ key: "bloom-colorful", src: "/bloom-colorful.png", labelKey: "settings.dock.iconColorful" },
	{ key: "bloom-golden", src: "/bloom-golden.png", labelKey: "settings.dock.iconGolden" },
	{ key: "bloom-biscuit", src: "/bloom-biscuit.png", labelKey: "settings.dock.iconOrange" },
	{ key: "windows", src: "/windows-11.png", labelKey: "settings.dock.iconWindows" }
];

const startIconTileStyle = (active: boolean): CSSProperties => ({
	width: "48px",
	height: "48px",
	borderRadius: "12px",
	border: active ? "2px solid var(--bloom-accent, #007aff)" : "2px solid rgba(255,255,255,0.1)",
	background: active ? "rgba(0,122,255,0.15)" : "rgba(255,255,255,0.05)",
	display: "flex",
	alignItems: "center",
	justifyContent: "center",
	cursor: "pointer",
	transition: "all 0.15s ease",
	padding: "6px",
	color: "rgba(255,255,255,0.5)"
});

interface DockTabProps {
	dockEnabled: boolean;
	toggleDock: () => void;
	dockMode: string;
	setDockModeValue: (mode: string) => void;
	dockEdgeDelay: number;
	handleDockEdgeDelayChange: (val: number) => void;
	dockPreviewEnabled: boolean;
	toggleDockPreview: () => void;
	dockShowBackgroundApps: boolean;
	toggleDockShowBackgroundApps: () => void;
	dockIconOnly: boolean;
	toggleDockIconOnly: () => void;
	dockSeparatorEnabled: boolean;
	toggleDockSeparator: () => void;
	dockAdaptive: boolean;
	toggleDockAdaptive: () => void;
	dockWinNumberEnabled: boolean;
	toggleDockWinNumber: () => void;
	dockExtrasPosition: string;
	handleDockExtrasPositionChange: (position: string) => void;
	dockExtrasDrives: boolean;
	toggleDockExtrasDrives: () => void;
	dockExtrasHiddenDrives: string;
	toggleDockExtrasDrive: (letter: string) => void;
	dockExtrasRecycleBin: boolean;
	toggleDockExtrasRecycleBin: () => void;
	dockTaskView: boolean;
	toggleDockTaskView: () => void;
	dockExtrasFolders: string;
	toggleDockExtraFolder: (id: string) => void;
	dockExtrasCustomFolders: string;
	handleAddDockExtraFolder: () => void;
	handleRemoveDockExtraFolder: (path: string) => void;
	startIcon: string;
	handleStartIconChange: (icon: string) => void;
}

export function DockTab({
	dockEnabled,
	toggleDock,
	dockMode,
	setDockModeValue,
	dockEdgeDelay,
	handleDockEdgeDelayChange,
	dockPreviewEnabled,
	toggleDockPreview,
	dockShowBackgroundApps,
	toggleDockShowBackgroundApps,
	dockIconOnly,
	toggleDockIconOnly,
	dockSeparatorEnabled,
	toggleDockSeparator,
	dockAdaptive,
	toggleDockAdaptive,
	dockWinNumberEnabled,
	toggleDockWinNumber,
	dockExtrasPosition,
	handleDockExtrasPositionChange,
	dockExtrasDrives,
	toggleDockExtrasDrives,
	dockExtrasHiddenDrives,
	toggleDockExtrasDrive,
	dockExtrasRecycleBin,
	toggleDockExtrasRecycleBin,
	dockTaskView,
	toggleDockTaskView,
	dockExtrasFolders,
	toggleDockExtraFolder,
	dockExtrasCustomFolders,
	handleAddDockExtraFolder,
	handleRemoveDockExtraFolder,
	startIcon,
	handleStartIconChange
}: DockTabProps) {
	useTranslation();

	const handleStartIconUpload = (e: ChangeEvent<HTMLInputElement>) => {
		const file = e.target.files?.[0];
		if (!file) return;
		const reader = new FileReader();
		reader.onload = () => {
			handleStartIconChange(`custom:${reader.result as string}`);
		};
		reader.readAsDataURL(file);
		e.target.value = "";
	};

	return (
		<>
			<div className="setting-group-label">{t("settings.groups.dock")}</div>
			<div className="setting-group">
				<SettingRow
					previewId="bloom-dock"
					icon={Monitor}
					label={t("settings.dock.enabled")}
					desc={t("settings.dock.enabledDesc")}
				>
					<label className="toggle-switch">
						<input type="checkbox" checked={dockEnabled} onChange={toggleDock} />
						<span className="slider"></span>
					</label>
				</SettingRow>

				{dockEnabled && (
					<>
						<SettingRow
							previewId="behavior"
							icon={dockMode === "fixed" ? EyeOff : Eye}
							label={t("settings.dock.behavior")}
							desc={t("settings.dock.behaviorDesc")}
						>
							<select
								className="settings-select"
								value={dockMode}
								onChange={(e) => setDockModeValue(e.target.value)}
							>
								{/* macOS can't reserve screen space for the dock. */}
								{!isMac && <option value="fixed">{t("common.behavior.fixed")}</option>}
								<option value="smart">{t("common.behavior.smart")}</option>
								<option value="peek">{t("common.behavior.peek")}</option>
							</select>
						</SettingRow>

						{dockMode !== "fixed" && (
							<SettingRow
								previewId="peek-delay"
								icon={Timer}
								label={t("settings.notch.peekDelay")}
								desc={
									dockEdgeDelay === 0
										? t("ui.dockTab.revealInstantlyOnBottomEdgeContact")
										: t("ui.dockTab.restOnTheBottomEdgeForDockedgedelayMsToReveal", {
												dockEdgeDelay: dockEdgeDelay
											})
								}
							>
								<input
									type="range"
									min="0"
									max="800"
									step="50"
									value={dockEdgeDelay}
									onChange={(e) => handleDockEdgeDelayChange(parseInt(e.target.value))}
									className="settings-slider"
								/>
							</SettingRow>
						)}

						<SettingRow
							previewId="show-app-previews"
							icon={Eye}
							label={t("settings.dock.previews")}
							desc={t("settings.dock.previewsDesc")}
						>
							<label className="toggle-switch">
								<input type="checkbox" checked={dockPreviewEnabled} onChange={toggleDockPreview} />
								<span className="slider"></span>
							</label>
						</SettingRow>

						<SettingRow
							previewId="show-background-apps"
							icon={Layers}
							label={t("ui.dockTab.showBackgroundApps")}
							desc={t("ui.dockTab.keepAppsInTheDockWhenTheyCloseToThe")}
						>
							<label className="toggle-switch">
								<input
									type="checkbox"
									aria-label={t("ui.dockTab.showBackgroundApps")}
									checked={dockShowBackgroundApps}
									onChange={toggleDockShowBackgroundApps}
								/>
								<span className="slider"></span>
							</label>
						</SettingRow>

						<SettingRow
							previewId="icon-only"
							icon={Circle}
							label={t("settings.dock.iconOnly")}
							desc={t("settings.dock.iconOnlyDesc")}
						>
							<label className="toggle-switch">
								<input type="checkbox" checked={dockIconOnly} onChange={toggleDockIconOnly} />
								<span className="slider"></span>
							</label>
						</SettingRow>

						<SettingRow
							previewId="app-separator"
							icon={Columns2}
							label={t("ui.dockTab.appSeparator")}
							desc={t("ui.dockTab.showADividerBeforeUnpinnedApps")}
						>
							<label className="toggle-switch">
								<input
									type="checkbox"
									checked={dockSeparatorEnabled}
									onChange={toggleDockSeparator}
								/>
								<span className="slider"></span>
							</label>
						</SettingRow>

						<SettingRow
							previewId="task-view"
							icon={Copy}
							label={t("ui.dock.taskView")}
							desc={t("ui.dockTab.showTheTaskViewButtonBesideStart")}
						>
							<label className="toggle-switch">
								<input type="checkbox" checked={dockTaskView} onChange={toggleDockTaskView} />
								<span className="slider"></span>
							</label>
						</SettingRow>

						<SettingRow
							previewId="win-number-shortcuts"
							icon={Keyboard}
							label={t("settings.dock.winNumber")}
							desc={t("settings.dock.winNumberDesc")}
							divider={dockMode === "fixed"}
						>
							<label className="toggle-switch">
								<input
									type="checkbox"
									checked={dockWinNumberEnabled}
									onChange={toggleDockWinNumber}
								/>
								<span className="slider"></span>
							</label>
						</SettingRow>

						{dockMode === "fixed" && (
							<SettingRow
								previewId="adaptive-mode"
								icon={Maximize2}
								label={t("settings.dock.adaptive")}
								desc={t("settings.dock.adaptiveDesc")}
								divider={false}
							>
								<label className="toggle-switch">
									<input type="checkbox" checked={dockAdaptive} onChange={toggleDockAdaptive} />
									<span className="slider"></span>
								</label>
							</SettingRow>
						)}

						<SettingRow
							previewId="dock-extras"
							icon={LayoutPanelLeft}
							label={t("ui.dockTab.dockExtras")}
							desc={t("ui.dockTab.showDrivesFoldersAndTheRecycleBinBesideTheApps")}
						>
							<select
								className="settings-select"
								value={dockExtrasPosition}
								onChange={(e) => handleDockExtrasPositionChange(e.target.value)}
							>
								<option value="off">{t("notch.commandCenter.off")}</option>
								<option value="left">{t("common.left")}</option>
								<option value="right">{t("common.right")}</option>
							</select>
						</SettingRow>

						{dockExtrasPosition !== "off" && (
							<details className="settings-disclosure">
								<summary>
									{t("ui.dockTab.manageDockExtras")}
									<span>{t("ui.dockTab.drivesFoldersAndRecycleBin")}</span>
								</summary>
								<SettingRow
									previewId="drives"
									icon={HardDrive}
									label={t("ui.dockTab.drives")}
									desc={t("ui.dockTab.fixedRemovableAndConnectedNetworkDrives")}
									divider={!dockExtrasDrives}
								>
									<label className="toggle-switch">
										<input
											type="checkbox"
											checked={dockExtrasDrives}
											onChange={toggleDockExtrasDrives}
										/>
										<span className="slider"></span>
									</label>
								</SettingRow>

								{dockExtrasDrives && (
									<>
										<div
											data-preview-id="dock-extras"
											className="setting-item"
											style={{ paddingTop: 0, paddingBottom: 12 }}
										>
											<DrivePicker
												hiddenDrives={dockExtrasHiddenDrives}
												onToggle={toggleDockExtrasDrive}
											/>
										</div>
										<div className="setting-divider" />
									</>
								)}

								<SettingRow
									previewId="recycle-bin"
									icon={Trash2}
									label={t("ui.dockTab.recycleBin")}
									desc={t("ui.dockTab.showTheRecycleBinInTheExtras")}
								>
									<label className="toggle-switch">
										<input
											type="checkbox"
											checked={dockExtrasRecycleBin}
											onChange={toggleDockExtrasRecycleBin}
										/>
										<span className="slider"></span>
									</label>
								</SettingRow>

								<div
									data-preview-id="dock-extras"
									className="setting-item"
									style={{ flexDirection: "column", alignItems: "flex-start", gap: "10px" }}
								>
									<div style={{ display: "flex", alignItems: "center", width: "100%" }}>
										<div className="setting-icon-bg">
											<FolderOpen size={14} strokeWidth={1.5} />
										</div>
										<div className="setting-info">
											<span className="setting-label">{t("ui.dockTab.specialFolders")}</span>
											<span className="setting-desc">
												{t("ui.dockTab.pickTheFoldersThatAppearInTheExtras")}
											</span>
										</div>
									</div>
									<div
										style={{
											display: "flex",
											gap: "8px",
											flexWrap: "wrap",
											paddingLeft: ROW_CONTENT_INDENT
										}}
									>
										{KNOWN_FOLDER_OPTIONS.map((folder) => {
											const active = parseEnabledFolders(dockExtrasFolders).includes(folder.id);
											return (
												<button
													key={folder.id}
													type="button"
													className={`folder-chip ${active ? "active" : ""}`}
													onClick={() => toggleDockExtraFolder(folder.id)}
												>
													{folder.label}
												</button>
											);
										})}
									</div>
								</div>
								<div className="setting-divider" />

								<div
									data-preview-id="dock-extras"
									className="setting-item"
									style={{ flexDirection: "column", alignItems: "flex-start", gap: "10px" }}
								>
									<div style={{ display: "flex", alignItems: "center", width: "100%" }}>
										<div className="setting-icon-bg">
											<FolderPlus size={14} strokeWidth={1.5} />
										</div>
										<div className="setting-info">
											<span className="setting-label">{t("ui.dockTab.customFolders")}</span>
											<span className="setting-desc">
												{t("ui.dockTab.addAnyFolderToTheExtrasSection")}
											</span>
										</div>
									</div>
									<div
										style={{
											display: "flex",
											flexDirection: "column",
											gap: "6px",
											width: "100%",
											paddingLeft: ROW_CONTENT_INDENT,
											boxSizing: "border-box"
										}}
									>
										{parseCustomFolders(dockExtrasCustomFolders).map((folder) => (
											<div key={folder.path} className="extra-folder-row">
												<span className="extra-folder-name">{folder.name}</span>
												<span className="extra-folder-path">{folder.path}</span>
												<button
													type="button"
													className="extra-folder-remove"
													title={t("ui.dockTab.removeFolder")}
													onClick={() => handleRemoveDockExtraFolder(folder.path)}
												>
													<X size={14} strokeWidth={2} />
												</button>
											</div>
										))}
										<button
											type="button"
											className="folder-chip add"
											onClick={handleAddDockExtraFolder}
										>
											<Plus size={13} strokeWidth={2} />
											{t("ui.dockTab.addFolder")}
										</button>
									</div>
								</div>
							</details>
						)}

						<div className="setting-divider" />
						<div
							data-preview-id="start-menu-icon"
							className="setting-item"
							style={{ flexDirection: "column", alignItems: "flex-start", gap: "10px" }}
						>
							<div style={{ display: "flex", alignItems: "center", width: "100%" }}>
								<div className="setting-icon-bg">
									<Sparkles size={14} strokeWidth={1.5} />
								</div>
								<div className="setting-info">
									<span className="setting-label">{t("settings.dock.startIcon")}</span>
									<span className="setting-desc">{t("settings.dock.startIconDesc")}</span>
								</div>
							</div>
							<div
								style={{
									display: "flex",
									gap: "8px",
									flexWrap: "wrap",
									paddingLeft: ROW_CONTENT_INDENT
								}}
							>
								{START_ICON_PRESETS.map((icon) => (
									<div
										key={icon.key}
										onClick={() => handleStartIconChange(icon.key)}
										style={startIconTileStyle(startIcon === icon.key)}
										title={t(icon.labelKey)}
									>
										<img
											src={icon.src}
											alt={t(icon.labelKey)}
											style={{ width: "100%", height: "100%", objectFit: "contain" }}
											draggable={false}
										/>
									</div>
								))}
								<div
									onClick={() => document.getElementById("start-icon-file-input")?.click()}
									style={startIconTileStyle(startIcon.startsWith("custom:"))}
									title={t("settings.dock.startIconCustom")}
								>
									{startIcon.startsWith("custom:") ? (
										<img
											src={startIcon.replace("custom:", "")}
											alt={t("settings.dock.startIconCustom")}
											style={{
												width: "100%",
												height: "100%",
												objectFit: "contain",
												borderRadius: "8px"
											}}
											draggable={false}
										/>
									) : (
										<Plus size={20} strokeWidth={2} />
									)}
								</div>
								{startIcon !== "default" && (
									<div
										onClick={() => handleStartIconChange("default")}
										style={startIconTileStyle(false)}
										title={t("settings.dock.startIconReset")}
									>
										<RotateCcw size={18} strokeWidth={1.5} />
									</div>
								)}
							</div>
							<input
								id="start-icon-file-input"
								type="file"
								accept=".png,.ico,.jpg,.jpeg,.svg,.bmp"
								style={{ display: "none" }}
								onChange={handleStartIconUpload}
							/>
						</div>
					</>
				)}
			</div>
		</>
	);
}
