import normalizer.intrinsics.N;

class ReceiptPick {
    static final ReceiptPick INSTANCE = new ReceiptPick(new Checkpoints());

    @N.External private final Checkpoints checkpoints;

    ReceiptPick(Checkpoints checkpoints) { this.checkpoints = checkpoints; }

    int choose(int[] values, int code) {
        int first = checkpoints.read(code);
        int second = checkpoints.read(first);
        int selected;
        /* @normalizer-ir selected = reduce values from second as (acc, item) yield acc * 2 + item */
        return selected;
    }
}

class Checkpoints { native int read(int code); }
