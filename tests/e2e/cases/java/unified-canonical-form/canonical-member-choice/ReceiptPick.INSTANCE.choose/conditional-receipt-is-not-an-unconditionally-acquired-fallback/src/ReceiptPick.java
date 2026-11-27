import normalizer.intrinsics.N;

class ReceiptPick {
    static final ReceiptPick INSTANCE = new ReceiptPick(new Checkpoints());

    @N.External private final Checkpoints checkpoints;

    ReceiptPick(Checkpoints checkpoints) { this.checkpoints = checkpoints; }

    int choose(int[] values, int code) {
        int first = code; if (code > 0) first = checkpoints.read(code);
        int second = checkpoints.read(first);
        int selected;
        /* @normalizer-ir selected = choose from values else second */
        return selected;
    }
}

class Checkpoints { native int read(int code); }
