package com.meowny.server.dto.categorygroup;

import com.fasterxml.jackson.databind.annotation.JsonDeserialize;
import com.meowny.server.config.HtmlSanitizationDeserializer;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.Size;

public record UpdateCategoryGroupRequest(
        @NotBlank(message = "Category group name is required")
        @Size(max = 50, message = "Category group name must be 50 characters or fewer")
        @JsonDeserialize(using = HtmlSanitizationDeserializer.class)
        String name
) {
}
