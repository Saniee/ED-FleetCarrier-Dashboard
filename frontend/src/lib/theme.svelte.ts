/**
 * Colour theme, kept in localStorage. app.html applies the stored theme before
 * first paint so a reload doesn't flash the default; this keeps it in sync after.
 */

export type Theme = 'dangerous' | 'alien';

const KEY = 'theme';
/** Long enough for the Alien power-on (body 0.5s, tracking band 0.3s + 1.2s). */
const BOOT_MS = 1600;
let bootTimer: ReturnType<typeof setTimeout> | undefined;

function load(): Theme {
	try {
		return localStorage.getItem(KEY) === 'alien' ? 'alien' : 'dangerous';
	} catch {
		return 'dangerous';
	}
}

let current = $state<Theme>(typeof window === 'undefined' ? 'dangerous' : load());

function apply(next: Theme) {
	current = next;
	const root = document.documentElement;
	clearTimeout(bootTimer);
	root.removeAttribute('data-boot');
	if (next === 'alien') {
		root.setAttribute('data-theme', 'alien');
		// Plays the power-on once, for this switch only; a reload restores the
		// theme in app.html without it.
		root.setAttribute('data-boot', '');
		bootTimer = setTimeout(() => root.removeAttribute('data-boot'), BOOT_MS);
	} else {
		root.removeAttribute('data-theme');
	}
	try {
		localStorage.setItem(KEY, next);
	} catch {
		// Not persisted; the theme still holds until reload.
	}
}

export const theme = {
	get current() {
		return current;
	},
	toggle() {
		apply(current === 'alien' ? 'dangerous' : 'alien');
	}
};
