record Task(int sequence) {}
class TaskBatch {
    static boolean complete(Task[] opened, Task[] closed) {
        int n = opened.length;
        int m = closed.length;
        int mismatches;
        /* @normalizer-ir
           mismatches = count from [0, min(n, m)) as i
                        where closed[i].sequence != opened[n - 1 - i].sequence
        */
        if (m > n) throw new IllegalArgumentException();
        if (mismatches > 0) throw new IllegalArgumentException();
        return m == n;
    }
}
