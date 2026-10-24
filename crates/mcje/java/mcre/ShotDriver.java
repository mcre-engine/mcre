package mcre;

import java.util.UUID;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.client.server.IntegratedServer;
import net.minecraft.client.renderer.texture.TextureAtlas;
import net.minecraft.client.renderer.texture.TextureAtlasSprite;
import net.minecraft.data.AtlasIds;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import net.minecraft.resources.Identifier;

/**
 * Render-thread steps for the Rust parity-shot tooling (tasks/vanilla_shot).
 *
 * <p>Instances are scheduled with {@code Minecraft.execute}, so {@link #run}
 * executes on the client render thread where GPU access is valid. The
 * {@code "fix"} step hands a server-authoritative teleport to the integrated
 * server thread.
 *
 * <p>Errors are reported through the volatile {@link #error} field instead of
 * being thrown, so a failed step never kills the render thread.
 */
public final class ShotDriver implements Runnable {
   public static volatile String error;
   public static volatile float waterVision;
   public static volatile double animationTicks;

   private final String step;
   private final String arg;
   private final double x;
   private final double y;
   private final double z;
   private final float yaw;
   private final float pitch;

   /**
    * Steps: {@code "hide"} hides the HUD, {@code "grab"} screenshots the main
    * render target to {@code screenshots/<arg>}, {@code "stop"} stops the
    * client.
    */
   public ShotDriver(final String step, final String arg) {
      this.step = step;
      this.arg = arg;
      this.x = 0.0;
      this.y = 0.0;
      this.z = 0.0;
      this.yaw = 0.0F;
      this.pitch = 0.0F;
   }

   /** Teleport the server player to feet {@code (x, y, z)} with a rotation. */
   public ShotDriver(final double x, final double y, final double z, final float yaw, final float pitch) {
      this.step = "fix";
      this.arg = null;
      this.x = x;
      this.y = y;
      this.z = z;
      this.yaw = yaw;
      this.pitch = pitch;
   }

   @Override
   public void run() {
      Minecraft minecraft = Minecraft.getInstance();

      try {
         switch (this.step) {
            case "hide":
               if (!minecraft.gui.hud.isHidden()) {
                  minecraft.gui.hud.toggle();
               }
               break;
            case "water-saturate": {
               if (minecraft.player == null) {
                  throw new IllegalStateException("no player");
               }
               Field waterVisionTime = minecraft.player.getClass().getDeclaredField("waterVisionTime");
               waterVisionTime.setAccessible(true);
               waterVisionTime.setInt(minecraft.player, 600);
               break;
            }
            case "grab":
               waterVision = minecraft.player.getWaterVision();
               animationTicks = waterAnimationTicks(minecraft);
               Screenshot.grab(minecraft.gameDirectory, this.arg, minecraft.gameRenderer.mainRenderTarget(), 1, message -> {});
               break;
            case "fix": {
               IntegratedServer server = minecraft.getSingleplayerServer();
               if (server == null) {
                  throw new IllegalStateException("no integrated server");
               }
               if (minecraft.player == null) {
                  throw new IllegalStateException("no player");
               }
               UUID id = minecraft.player.getUUID();
               double x = this.x;
               double y = this.y;
               double z = this.z;
               float yaw = this.yaw;
               float pitch = this.pitch;
               server.execute(() -> {
                  ServerPlayer player = server.getPlayerList().getPlayer(id);
                  if (player != null) {
                     ServerGamePacketListenerImpl connection = player.connection;
                     connection.teleport(x, y, z, yaw, pitch);
                  }
               });
               break;
            }
            case "stop":
               minecraft.stop();
               break;
            default:
               throw new IllegalArgumentException("unknown step: " + this.step);
         }
      } catch (Throwable t) {
         error = t.toString();
      }
   }

   private static double waterAnimationTicks(final Minecraft minecraft) {
      try {
         TextureAtlas atlas = minecraft.getAtlasManager().getAtlasOrThrow(AtlasIds.BLOCKS);
         TextureAtlasSprite sprite = atlas.getSprite(Identifier.withDefaultNamespace("block/water_still"));
         Field statesField = TextureAtlas.class.getDeclaredField("animatedTexturesStates");
         Field spritesField = TextureAtlas.class.getDeclaredField("sprites");
         statesField.setAccessible(true);
         spritesField.setAccessible(true);
         List<?> states = (List<?>)statesField.get(atlas);
         int animationIndex = 0;
         for (Object value : (List<?>)spritesField.get(atlas)) {
            TextureAtlasSprite candidate = (TextureAtlasSprite)value;
            if (!candidate.isAnimated()) {
               continue;
            }
            if (candidate != sprite) {
               animationIndex++;
               continue;
            }
            Object state = states.get(animationIndex);
            Field frameField = state.getClass().getDeclaredField("frame");
            Field subFrameField = state.getClass().getDeclaredField("subFrame");
            Field infoField = state.getClass().getDeclaredField("animationInfo");
            frameField.setAccessible(true);
            subFrameField.setAccessible(true);
            infoField.setAccessible(true);
            int currentFrame = frameField.getInt(state);
            double ticks = subFrameField.getInt(state);
            Object info = infoField.get(state);
            Field framesField = info.getClass().getDeclaredField("frames");
            framesField.setAccessible(true);
            List<?> frames = (List<?>)framesField.get(info);
            Method timeMethod = frames.get(0).getClass().getDeclaredMethod("time");
            timeMethod.setAccessible(true);
            for (int frame = 0; frame < currentFrame; frame++) {
               ticks += ((Number)timeMethod.invoke(frames.get(frame))).intValue();
            }
            return ticks;
         }
      } catch (ReflectiveOperationException | RuntimeException error) {
         System.err.println("Could not read water animation clock: " + error);
      }
      return 0.0;
   }
}
