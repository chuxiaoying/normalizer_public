import normalizer.intrinsics.N;
class ChoiceBase { int value; }
class StatePick {
    static final StatePick INSTANCE = new StatePick();

    @N.EntryState ChoiceBase seed = new ChoiceBase();
    int choose(int[] values, int ignored) {
        int fallback = seed.value;
        int selected;
        /* @normalizer-ir selected = choose from values else fallback */
        return selected;
    }
}
