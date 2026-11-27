import normalizer.intrinsics.N;

class ReceiptPick {
    static final ReceiptPick INSTANCE = new ReceiptPick(new Checkpoints());

    @N.External private final Checkpoints checkpoints;

    ReceiptPick(Checkpoints checkpoints) { this.checkpoints = checkpoints; }

    int choose(int[] values, int code) {
        int first = checkpoints.read(code);
        int observed = checkpoints.read(first);
        int selected;
        /* @normalizer-ir selected = choose from values else observed */
        return selected;
    }
}

class Checkpoints { native int read(int code); }
