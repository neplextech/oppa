export function playPrinterSound() {
  const audio = new Audio('/receipt_printer_audio.mp3');
  void audio.play().catch(() => undefined);
}
