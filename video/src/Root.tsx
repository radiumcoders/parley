import "./index.css";
import { Composition } from "remotion";
import { TOTAL } from "./lib/cues";
import { FPS } from "./theme";
import { Video } from "./Video";

export const RemotionRoot: React.FC = () => (
  <Composition id="Parley" component={Video} durationInFrames={TOTAL} fps={FPS} width={1280} height={720} />
);
