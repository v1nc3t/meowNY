package com.meowny.server.dto.transaction;

import com.fasterxml.jackson.databind.annotation.JsonDeserialize;
import com.meowny.server.config.HtmlSanitizationDeserializer;
import jakarta.validation.constraints.*;

import java.math.BigDecimal;
import java.time.LocalDate;

public record UpdateTransactionRequest(
        @NotNull(message = "Category ID is required")
        Long categoryId,

        @NotBlank(message = "Transaction name is required")
        @Size(max = 50, message = "Transaction name must be 50 characters or fewer")
        @JsonDeserialize(using = HtmlSanitizationDeserializer.class)
        String name,

        @NotNull(message = "Amount is required")
        @PositiveOrZero(message = "Amount must be zero or positive")
        @Digits(integer = 12, fraction = 2)
        BigDecimal amount,

        @NotNull(message = "Payment date is required")
        @PastOrPresent(message = "Payment date cannot be in the future")
        LocalDate paymentDate,

        @Size(max = 100, message = "Description must be 100 characters or fewer")
        @JsonDeserialize(using = HtmlSanitizationDeserializer.class)
        String description
) {
}
