package com.meowny.server.dto.category;

import com.meowny.commons.entity.TransactionType;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Size;

public record CreateCategoryRequest(
        @NotNull(message = "User ID is required")
        Long userId,

        @NotNull(message = "Transaction type is required")
        TransactionType type,

        @NotBlank(message = "Category name is required")
        @Size(max = 50, message = "Category name must be 50 characters or fewer")
        String name
) {
}
