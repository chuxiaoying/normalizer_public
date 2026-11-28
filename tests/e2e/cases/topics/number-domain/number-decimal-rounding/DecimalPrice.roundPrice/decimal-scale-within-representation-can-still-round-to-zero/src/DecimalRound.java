import java.math.BigDecimal;
import normalizer.intrinsics.N;

class DecimalRound {
    static BigDecimal run() {
        BigDecimal value;
        /* @normalizer-ir value = 0.1 */
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        value = N.multiply(value, value);
        return N.round(value, 0, N.HALF_EVEN);
    }
}
