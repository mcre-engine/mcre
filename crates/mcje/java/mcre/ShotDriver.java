package mcre;

import java.util.UUID;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.client.server.IntegratedServer;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.network.ServerGamePacketListenerImpl;

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
            case "grab":
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
}
