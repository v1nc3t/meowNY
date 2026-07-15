package com.meowny.server.dto.recurringTransaction;

import com.meowny.commons.entity.Frequency;
import jakarta.validation.constraints.*;

import java.math.BigDecimal;
import java.time.LocalDate;

public record UpdateRecurringTransactionRequest(
        @NotNull(message = "Category ID is required")
        Long categoryId,

        @NotBlank(message = "Template name is required")
        @Size(max = 50, message = "Template name must be 50 characters or fewer")
        String name,

        @NotNull(message = "Amount is required")
        @PositiveOrZero(message = "Amount must be zero or positive")
        @Digits(integer = 12, fraction = 2)
        BigDecimal amount,

        @NotNull(message = "Next due date is required")
        @FutureOrPresent(message = "The next due date must be today or in the future")
        LocalDate nextDueDate,

        @NotNull(message = "Frequency is required")
        Frequency frequency,

        @NotNull(message = "Active status is required")
        Boolean isActive
) {
}
