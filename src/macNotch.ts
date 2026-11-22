import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { isMac } from "./platform";

/** The camera housing at the top of a Mac screen, in points. */
export type MacNotch = { x: number; width: number; height: number };

/** The main screen's notch on a Mac that has one; null everywhere else. */
export function useMacNotch(): MacNotch | null {
	const [notch, setNotch] = useState<MacNotch | null>(null);
	useEffect(() => {
		if (!isMac) return;
		invoke<MacNotch | null>("get_notch")
			.then(setNotch)
			.catch((error) => console.error("get_notch failed", error));
	}, []);
	return notch;
}
