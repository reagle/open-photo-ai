/** The two notes of the chime, in hertz: a rising fifth, which reads as "done" rather than as an alert. */
const NOTES = [660, 990];
/** How long each note sounds, in seconds. */
const NOTE_S = 0.18;
/** How loud, against the system's full scale: a notice, not an alarm. */
const VOLUME = 0.12;

/**
 * Plays a short chime, as a finished batch announces itself: synthesised, so the application ships no sound file and
 * asks for no permission. Silent where the webview has no audio - jsdom, or an audio context it refuses.
 */
export const chime = () => {
    const Context = typeof window === "undefined" ? undefined : window.AudioContext;
    if (Context === undefined) return;

    try {
        const context = new Context();
        NOTES.forEach((frequency, index) => {
            const start = context.currentTime + index * NOTE_S;
            const oscillator = context.createOscillator();
            const gain = context.createGain();

            oscillator.type = "sine";
            oscillator.frequency.value = frequency;
            // A quick rise and an exponential fall, so neither end clicks.
            gain.gain.setValueAtTime(0.0001, start);
            gain.gain.exponentialRampToValueAtTime(VOLUME, start + 0.02);
            gain.gain.exponentialRampToValueAtTime(0.0001, start + NOTE_S * 2);
            oscillator.connect(gain).connect(context.destination);
            oscillator.start(start);
            oscillator.stop(start + NOTE_S * 2);
        });
        setTimeout(() => void context.close().catch(() => {}), (NOTES.length + 2) * NOTE_S * 1000);
    } catch {
        // No audio is no reason to fail the notice it goes with.
    }
};
