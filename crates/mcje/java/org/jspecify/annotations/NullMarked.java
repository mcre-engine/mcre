package org.jspecify.annotations;

import java.lang.annotation.ElementType;
import java.lang.annotation.Target;

@Target({
   ElementType.MODULE,
   ElementType.PACKAGE,
   ElementType.TYPE,
   ElementType.METHOD,
   ElementType.CONSTRUCTOR,
   ElementType.FIELD
})
public @interface NullMarked {
}
