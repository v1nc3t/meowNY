package commons;

import org.junit.jupiter.api.*;

import static org.junit.jupiter.api.Assertions.*;

import java.time.LocalDate;

public class FrequencyTest {

    private final LocalDate start = LocalDate.of(2026, 1, 1);

    @Test
    void dailyTest() {
        assertEquals(
                LocalDate.of(2026, 1, 2),
                Frequency.DAILY.nextDate(start)
        );
    }

    @Test
    void weeklyTest() {
        assertEquals(
                LocalDate.of(2026, 1, 8),
                Frequency.WEEKLY.nextDate(start)
        );
    }

    @Test
    void biweeklyTest() {
        assertEquals(
                LocalDate.of(2026, 1, 15),
                Frequency.BIWEEKLY.nextDate(start)
        );
    }

    @Test
    void triweeklyTest() {
        assertEquals(
                LocalDate.of(2026, 1, 22),
                Frequency.TRIWEEKLY.nextDate(start)
        );
    }

    @Test
    void monthlyTest() {
        assertEquals(
                LocalDate.of(2026, 2, 1),
                Frequency.MONTHLY.nextDate(start)
        );
    }

    @Test
    void bimonthlyTest() {
        assertEquals(
                LocalDate.of(2026, 3, 1),
                Frequency.BIMONTHLY.nextDate(start)
        );
    }

    @Test
    void quarterlyTest() {
        assertEquals(
                LocalDate.of(2026, 4, 1),
                Frequency.QUARTERLY.nextDate(start)
        );
    }

    @Test
    void yearlyTest() {
        assertEquals(
                LocalDate.of(2027, 1, 1),
                Frequency.YEARLY.nextDate(start)
        );
    }

}
