/**
 * Whether to play animations even when the OS asks for reduced motion. Kept in
 * localStorage, like the theme, because reduced motion is itself a per-device
 * setting. app.html applies the stored choice before first paint; global.scss
 * only stills animations while `data-motion="full"` is absent.
 */

const KEY = 'motion';

function load(): boolean {
	try {
		return localStorage.getItem(KEY) === 'full';
	} catch {
		return false;
	}
}

let full = $state(typeof window === 'undefined' ? false : load());

export const motion = {
	get full() {
		return full;
	},
	/** Whether the OS currently asks for reduced motion. */
	get systemReduced() {
		return typeof window !== 'undefined' && matchMedia('(prefers-reduced-motion: reduce)').matches;
	},
	set(next: boolean) {
		full = next;
		if (next) document.documentElement.setAttribute('data-motion', 'full');
		else document.documentElement.removeAttribute('data-motion');
		try {
			if (next) localStorage.setItem(KEY, 'full');
			else localStorage.removeItem(KEY);
		} catch {
			// Not persisted; the choice still holds until reload.
		}
	}
};
