/**
 * Location access for the app — the ONLY place that touches the GPS. Every
 * location read goes through here so permission handling and the "we only read
 * it when you ask" promise live in one auditable spot.
 *
 * Foreground / when-in-use only (background cadence is a later roadmap item).
 */
import * as Location from 'expo-location';

export type Fix = { lat: number; lon: number; acc_m?: number };
export type PermState = 'granted' | 'denied' | 'undetermined';

function map(status: Location.PermissionStatus): PermState {
  if (status === Location.PermissionStatus.GRANTED) return 'granted';
  if (status === Location.PermissionStatus.DENIED) return 'denied';
  return 'undetermined';
}

/** Current permission WITHOUT prompting — for gating the UI. */
export async function getPermission(): Promise<PermState> {
  try {
    const { status } = await Location.getForegroundPermissionsAsync();
    return map(status);
  } catch {
    return 'undetermined';
  }
}

/** Ask for permission (shows the OS prompt the first time). */
export async function requestPermission(): Promise<PermState> {
  const { status } = await Location.requestForegroundPermissionsAsync();
  return map(status);
}

/**
 * Get one location fix, requesting permission if needed. Throws an Error whose
 * message is safe to show the user (e.g. permission denied, or no signal).
 */
export async function getFix(): Promise<Fix> {
  let perm = await getPermission();
  if (perm !== 'granted') perm = await requestPermission();
  if (perm === 'denied') {
    throw new Error(
      'Location permission is off. Enable it for Lifeline in your device Settings to share where you are.'
    );
  }
  if (perm !== 'granted') {
    throw new Error('Location permission was not granted.');
  }
  const pos = await Location.getCurrentPositionAsync({
    accuracy: Location.Accuracy.Balanced,
  });
  return {
    lat: pos.coords.latitude,
    lon: pos.coords.longitude,
    acc_m: pos.coords.accuracy ?? undefined,
  };
}
