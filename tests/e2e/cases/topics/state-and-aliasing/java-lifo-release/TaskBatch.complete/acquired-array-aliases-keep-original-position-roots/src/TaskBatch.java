record Task(int id) {}
class TaskBatch {
    static boolean complete(Task[] opened, Task[] closed) { Task[] incoming = opened; Task[] outgoing = closed;
        int n = incoming.length;
        int m = outgoing.length;
        int mismatches;
        /* @normalizer-ir
           mismatches = count from [0, min(n, m)) as i
                        where outgoing[i].id != incoming[n - 1 - i].id
        */
        if (m > n) throw new IllegalArgumentException();
        if (mismatches > 0) throw new IllegalArgumentException();
        return m == n;
    }
}
