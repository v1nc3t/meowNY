package com.meowny.server.dto.transaction;

import com.meowny.commons.entity.TransactionType;
import jakarta.validation.constraints.*;

import java.math.BigDecimal;
import java.time.LocalDate;

public record CreateTransactionRequest (
        @NotNull(message = "User ID is required")
        Long userId,

        @NotNull(message = "Category ID is required")
        Long categoryId,

        // Optional field: null if manually logged, populated if spawned from a template
        Long recurringTransactionId,

        @NotNull(message = "Transaction type is required")
        TransactionType type,

        @NotBlank(message = "Transaction name is required")
        @Size(max = 50, message = "Transaction name must be 50 characters or fewer")
        String name,

        @NotNull(message = "Amount is required")
        @PositiveOrZero(message = "Amount must be zero or positive")
        @Digits(integer = 12, fraction = 2)
        BigDecimal amount,

        @NotNull(message = "Payment date is required")
        @PastOrPresent(message = "Payment date cannot be in the future")
        LocalDate paymentDate,

        @Size(max = 100, message = "Description must be 100 characters or fewer")
        String description
) {
}
