class InstanceEntry {
    static final InstanceEntry INSTANCE = new InstanceEntry();

    int add(int value, int fee) {
        return value + fee;
    }
}
